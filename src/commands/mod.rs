//! Command dispatch and shared execution context.

mod api;
mod auth;
mod channel;
mod file;
mod message;
mod reaction;
mod search;
mod user;

use std::io::{IsTerminal, Write};

use crate::cli::{Args, Cmd};
use crate::config::{Config, Workspace};
use crate::error::{Error, Result};
use crate::resolve::Directory;
use crate::slack::Client;

/// Everything a command handler needs.
pub struct Ctx {
    pub client: Client,
    pub workspace: Workspace,
    pub workspace_name: String,
    pub json: bool,
    pub no_cache: bool,
}

impl Ctx {
    fn new(args: &Args) -> Result<Ctx> {
        let cfg = Config::load()?;
        let (workspace_name, workspace) = cfg.resolve(args.workspace.as_deref())?;
        let client = Client::new(&workspace, args.verbose)?;
        Ok(Ctx {
            client,
            workspace,
            workspace_name,
            json: args.json,
            no_cache: args.no_cache,
        })
    }

    pub fn dir(&self) -> Directory<'_> {
        Directory::new(&self.client, &self.workspace_name, self.no_cache)
    }
}

/// Parse, dispatch, run.
pub fn run(args: Args) -> Result<()> {
    match &args.cmd {
        Cmd::Auth { cmd } => auth::run(cmd, &args),
        _ => {
            let ctx = Ctx::new(&args)?;
            match args.cmd {
                Cmd::Auth { .. } => unreachable!("handled above"),
                Cmd::Channel { cmd } => channel::run(&ctx, cmd),
                Cmd::Message { cmd } => message::run(&ctx, cmd),
                Cmd::Search { cmd } => search::run(&ctx, cmd),
                Cmd::User { cmd } => user::run(&ctx, cmd),
                Cmd::Reaction { cmd } => reaction::run(&ctx, cmd),
                Cmd::File { cmd } => file::run(&ctx, cmd),
                Cmd::Api {
                    method,
                    params,
                    data,
                } => api::run(&ctx, &method, &params, data.as_deref()),
            }
        }
    }
}

/// Confirm a destructive action: prompt on a terminal, require --yes otherwise.
pub fn confirm(prompt: &str, yes: bool) -> Result<()> {
    if yes {
        return Ok(());
    }
    if !std::io::stdin().is_terminal() {
        return Err(Error::Usage(
            "refusing destructive action without --yes in non-interactive use".into(),
        ));
    }
    eprint!("{prompt} [y/N] ");
    std::io::stderr().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let ok = matches!(line.trim().to_lowercase().as_str(), "y" | "yes");
    if ok {
        Ok(())
    } else {
        Err(Error::Usage("aborted".into()))
    }
}
