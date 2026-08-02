//! Profile management: add, list, status, use, remove.

use std::io::{BufRead, IsTerminal};

use chrono::Utc;
use serde_json::Value;

use crate::cli::{Args, AuthCmd};
use crate::config::{Config, Profile, TokenKind};
use crate::error::{Error, Result};
use crate::output::print_json;
use crate::slack::Client;

pub fn run(cmd: &AuthCmd, args: &Args) -> Result<()> {
    match cmd {
        AuthCmd::Add {
            name,
            token_stdin,
            curl,
        } => add(name, *token_stdin, *curl, args.json),
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

/// Read a copied cURL command from piped stdin or the macOS clipboard.
fn read_curl() -> Result<String> {
    use std::io::Read;
    let stdin = std::io::stdin();
    if !stdin.is_terminal() {
        let mut buf = String::new();
        stdin.lock().read_to_string(&mut buf)?;
        return Ok(buf);
    }

    #[cfg(target_os = "macos")]
    return read_curl_from_clipboard(&stdin);

    #[cfg(not(target_os = "macos"))]
    read_curl_from_terminal(&stdin)
}

#[cfg(target_os = "macos")]
fn read_curl_from_clipboard(stdin: &std::io::Stdin) -> Result<String> {
    use std::process::Command;

    eprintln!(concat!(
        "Import a signed-in Slack browser session:\n",
        "\n",
        "1. Open Slack in your browser and open Developer Tools.\n",
        "2. Select the Network tab, then reload Slack or open a channel.\n",
        "3. Select any request whose URL contains /api/.\n",
        "4. Right-click it and choose Copy > Copy as cURL.\n",
        "   Do not use Copy as fetch; it does not include the required cookie.\n",
        "5. Return here and press Enter. Do not paste the command.\n",
        "\n",
        "The copied request will be read from your clipboard. Your token and cookie\n",
        "will not be printed.\n",
        "Press Enter when ready:"
    ));

    let mut ready = String::new();
    stdin.lock().read_line(&mut ready)?;

    let output = Command::new("pbpaste").output().map_err(|error| {
        Error::Usage(format!(
            "could not read the macOS clipboard with pbpaste: {error}"
        ))
    })?;
    if !output.status.success() {
        return Err(Error::Usage(
            "could not read the macOS clipboard with pbpaste".into(),
        ));
    }
    let pasted = String::from_utf8(output.stdout)
        .map_err(|_| Error::Usage("the clipboard does not contain valid text".into()))?;
    if pasted.trim().is_empty() {
        return Err(Error::Usage(
            "the clipboard is empty — use Copy > Copy as cURL in Slack, then try again".into(),
        ));
    }
    Ok(pasted)
}

#[cfg(not(target_os = "macos"))]
fn read_curl_from_terminal(stdin: &std::io::Stdin) -> Result<String> {
    eprintln!("paste the 'Copy as cURL' command, then press Enter on a blank line:");
    let mut buf = String::new();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() && !buf.trim().is_empty() {
            break;
        }
        buf.push_str(&line);
        buf.push('\n');
    }
    Ok(buf)
}

fn add(name: &str, token_stdin: bool, curl: bool, json: bool) -> Result<()> {
    let (token, mut cookie) = if curl {
        let pasted = read_curl()?;
        let pair = super::curl::parse(&pasted)?;
        (pair.token, Some(pair.cookie))
    } else if token_stdin {
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
        (read_secret("token: ")?, None)
    };

    let kind = TokenKind::of(&token);
    if kind == TokenKind::Unknown {
        return Err(Error::Usage(
            "unrecognized token prefix — expected xoxb-, xoxp-, or xoxc-".into(),
        ));
    }
    if kind == TokenKind::Session && cookie.is_none() {
        if !token_stdin && !curl {
            cookie = Some(read_secret("d cookie (xoxd-…): ")?);
        }
        if cookie.as_deref().unwrap_or("").is_empty() {
            return Err(Error::Usage(
                "session (xoxc) tokens require the d cookie (line 2 on stdin)".into(),
            ));
        }
    }

    let mut profile = Profile {
        token,
        cookie,
        team: None,
        team_id: None,
        user: None,
        user_id: None,
        url: None,
        validated_at: None,
    };
    let client = Client::new(&profile, false)?;
    let v = client.call("auth.test", &[])?;
    let field = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    profile.team = field("team");
    profile.team_id = field("team_id");
    profile.user = field("user");
    profile.user_id = field("user_id");
    profile.url = field("url");
    profile.validated_at = Some(Utc::now().to_rfc3339());

    let mut cfg = Config::load()?;
    let first = cfg.profiles.is_empty();
    let identity = describe(&profile);
    cfg.profiles.insert(name.to_string(), profile);
    if first {
        cfg.default_profile = Some(name.to_string());
    }
    cfg.save()?;

    if json {
        print_json(&v);
    } else {
        println!("added profile '{name}': {identity}");
        if first {
            println!("set as default profile");
        }
    }
    Ok(())
}

fn describe(p: &Profile) -> String {
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
            .profiles
            .iter()
            .map(|(name, p)| {
                serde_json::json!({
                    "name": name,
                    "default": cfg.default_profile.as_deref() == Some(name),
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
    if cfg.profiles.is_empty() {
        println!("no profiles — run `slack auth add <name>`");
        return Ok(());
    }
    for (name, p) in &cfg.profiles {
        let marker = if cfg.default_profile.as_deref() == Some(name) {
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
    let (resolved, profile) = cfg.resolve(name.or(args.profile.as_deref()))?;
    let client = Client::new(&profile, args.verbose)?;
    let v = client.call("auth.test", &[])?;
    if args.json {
        print_json(&v);
        return Ok(());
    }
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("?");
    println!(
        "profile '{resolved}': {} ({}) in {} [{} token] — {}",
        s("user"),
        s("user_id"),
        s("team"),
        profile.kind().label(),
        s("url"),
    );
    Ok(())
}

fn set_default(name: &str) -> Result<()> {
    let mut cfg = Config::load()?;
    if !cfg.profiles.contains_key(name) {
        return Err(Error::Auth(format!("profile not found: {name}")));
    }
    cfg.default_profile = Some(name.to_string());
    cfg.save()?;
    println!("default profile: {name}");
    Ok(())
}

fn remove(name: &str, yes: bool) -> Result<()> {
    let mut cfg = Config::load()?;
    if !cfg.profiles.contains_key(name) {
        return Err(Error::Auth(format!("profile not found: {name}")));
    }
    super::confirm(&format!("remove profile '{name}'?"), yes)?;
    cfg.profiles.remove(name);
    if cfg.default_profile.as_deref() == Some(name) {
        cfg.default_profile = None;
    }
    cfg.save()?;
    println!("removed profile '{name}'");
    Ok(())
}
