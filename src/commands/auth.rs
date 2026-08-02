//! Workspace management: add, list, status, use, remove.

use std::io::{BufRead, IsTerminal};

use chrono::Utc;
use serde_json::Value;

use crate::cli::{Args, AuthCmd};
use crate::config::{Config, TokenKind, Workspace};
use crate::error::{Error, Result};
use crate::output::print_json;
use crate::slack::Client;

pub fn run(cmd: &AuthCmd, args: &Args) -> Result<()> {
    match cmd {
        AuthCmd::Add { name, token_stdin } => add(name, *token_stdin, args.json),
        AuthCmd::List => list(args.json),
        AuthCmd::Status { name } => status(name.as_deref(), args),
        AuthCmd::Use { name } => set_default(name),
        AuthCmd::Remove { name, yes } => remove(name, *yes),
    }
}

fn read_secret(prompt: &str) -> Result<String> {
    let value = if std::io::stdin().is_terminal() {
        rpassword::prompt_password(prompt).map_err(Error::Io)?
    } else {
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        line
    };
    Ok(value.trim().to_string())
}

fn add(name: &str, token_stdin: bool, json: bool) -> Result<()> {
    let (token, mut cookie) = if token_stdin {
        let stdin = std::io::stdin();
        let mut lines = stdin.lock().lines();
        let token = lines
            .next()
            .transpose()?
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .ok_or_else(|| Error::Usage("expected token on stdin line 1".into()))?;
        let cookie = lines
            .next()
            .transpose()?
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty());
        (token, cookie)
    } else {
        (
            read_secret("token (xoxp-/xoxb-, or xoxc- from Network > Payload): ")?,
            None,
        )
    };

    let kind = TokenKind::of(&token);
    if kind == TokenKind::Unknown {
        return Err(Error::Usage(
            "unrecognized token prefix — expected xoxb-, xoxp-, or xoxc-".into(),
        ));
    }
    if kind == TokenKind::Session && cookie.is_none() {
        if !token_stdin {
            eprintln!(concat!(
                "Browser sessions also need the d cookie:\n",
                "1. In DevTools, open Application > Cookies.\n",
                "2. Select your Slack workspace and copy the value of the d cookie.\n"
            ));
            cookie = Some(read_secret("d cookie (xoxd-…): ")?);
        }
        if cookie.as_deref().unwrap_or("").is_empty() {
            return Err(Error::Usage(
                "session (xoxc) tokens require the d cookie (line 2 on stdin)".into(),
            ));
        }
    }

    let mut workspace = Workspace {
        token,
        cookie,
        team: None,
        team_id: None,
        user: None,
        user_id: None,
        url: None,
        validated_at: None,
    };
    let client = Client::new(&workspace, false)?;
    let v = client.call("auth.test", &[])?;
    let field = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    workspace.team = field("team");
    workspace.team_id = field("team_id");
    workspace.user = field("user");
    workspace.user_id = field("user_id");
    workspace.url = field("url");
    workspace.validated_at = Some(Utc::now().to_rfc3339());

    let mut cfg = Config::load()?;
    let first = cfg.workspaces.is_empty();
    let identity = describe(&workspace);
    cfg.workspaces.insert(name.to_string(), workspace);
    if first {
        cfg.default_workspace = Some(name.to_string());
    }
    cfg.save()?;

    if json {
        print_json(&v);
    } else {
        println!("added workspace '{name}': {identity}");
        if first {
            println!("set as default workspace");
        }
    }
    Ok(())
}

fn describe(p: &Workspace) -> String {
    format!(
        "{} {} ({}) in {} [{} token]",
        if p.kind().acts_as_member() {
            "user"
        } else {
            "bot"
        },
        p.user.as_deref().unwrap_or("?"),
        p.user_id.as_deref().unwrap_or("?"),
        p.team.as_deref().unwrap_or("?"),
        p.kind().label(),
    )
}

fn list(json: bool) -> Result<()> {
    let cfg = Config::load()?;
    if json {
        let rows: Vec<Value> = cfg
            .workspaces
            .iter()
            .map(|(name, p)| {
                serde_json::json!({
                    "name": name,
                    "default": cfg.default_workspace.as_deref() == Some(name),
                    "kind": p.kind().label(),
                    "team": p.team,
                    "team_id": p.team_id,
                    "user": p.user,
                    "user_id": p.user_id,
                })
            })
            .collect();
        print_json(&Value::Array(rows));
        return Ok(());
    }
    if cfg.workspaces.is_empty() {
        println!("no workspaces — run `slack auth add <name>`");
        return Ok(());
    }
    for (name, p) in &cfg.workspaces {
        let marker = if cfg.default_workspace.as_deref() == Some(name) {
            "*"
        } else {
            " "
        };
        println!("{marker} {name}  {}", describe(p));
    }
    Ok(())
}

fn status(name: Option<&str>, args: &Args) -> Result<()> {
    let cfg = Config::load()?;
    let (resolved, workspace) = cfg.resolve(name.or(args.workspace.as_deref()))?;
    let client = Client::new(&workspace, args.verbose)?;
    let v = client.call("auth.test", &[])?;
    if args.json {
        print_json(&v);
        return Ok(());
    }
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("?");
    println!(
        "workspace '{resolved}': {} ({}) in {} [{} token] — {}",
        s("user"),
        s("user_id"),
        s("team"),
        workspace.kind().label(),
        s("url"),
    );
    Ok(())
}

fn set_default(name: &str) -> Result<()> {
    let mut cfg = Config::load()?;
    if !cfg.workspaces.contains_key(name) {
        return Err(Error::Auth(format!("workspace not found: {name}")));
    }
    cfg.default_workspace = Some(name.to_string());
    cfg.save()?;
    println!("default workspace: {name}");
    Ok(())
}

fn remove(name: &str, yes: bool) -> Result<()> {
    let mut cfg = Config::load()?;
    if !cfg.workspaces.contains_key(name) {
        return Err(Error::Auth(format!("workspace not found: {name}")));
    }
    super::confirm(&format!("remove workspace '{name}'?"), yes)?;
    cfg.workspaces.remove(name);
    if cfg.default_workspace.as_deref() == Some(name) {
        cfg.default_workspace = None;
    }
    cfg.save()?;
    println!("removed workspace '{name}'");
    Ok(())
}
