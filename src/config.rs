//! Workspace storage and precedence.
//!
//! Tokens live in `~/.config/slack/config.json` at mode 0600 by deliberate
//! choice (plan amendment 2): the OS keychain re-binds to the binary signature
//! on every `cargo install`, which makes a rebuilt personal tool re-prompt
//! constantly. 0600 in the home directory matches gspace/xio/coresignal.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Token kind, derived from the token prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// `xoxb-` — installed app acting as itself.
    Bot,
    /// `xoxp-` — user token from a private app; acts as the installing member.
    User,
    /// `xoxc-` + `d` cookie — browser session; acts as the member, unsanctioned.
    Session,
    Unknown,
}

impl TokenKind {
    pub fn of(token: &str) -> TokenKind {
        if token.starts_with("xoxb-") {
            TokenKind::Bot
        } else if token.starts_with("xoxp-") {
            TokenKind::User
        } else if token.starts_with("xoxc-") {
            TokenKind::Session
        } else {
            TokenKind::Unknown
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            TokenKind::Bot => "bot",
            TokenKind::User => "user",
            TokenKind::Session => "session",
            TokenKind::Unknown => "unknown",
        }
    }

    /// Whether this token posts as the authenticated member.
    pub fn acts_as_member(self) -> bool {
        matches!(self, TokenKind::User | TokenKind::Session)
    }
}

/// One stored workspace credential plus non-secret identity metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub token: String,
    /// `d` cookie value; required when the token is `xoxc-`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cookie: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Last successful `auth.test`, RFC 3339.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validated_at: Option<String>,
}

impl Workspace {
    pub fn kind(&self) -> TokenKind {
        TokenKind::of(&self.token)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(
        default,
        alias = "default_profile",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_workspace: Option<String>,
    #[serde(default, alias = "profiles")]
    pub workspaces: BTreeMap<String, Workspace>,
}

/// `~/.config/slack` — config plus per-workspace directory caches.
pub fn config_dir() -> Result<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| Error::Auth("cannot determine home directory".into()))?;
    Ok(home.join(".config").join("slack"))
}

fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.json"))
}

impl Config {
    /// Load the config, returning defaults when no file exists yet.
    pub fn load() -> Result<Config> {
        match fs::read_to_string(config_path()?) {
            Ok(s) => Ok(serde_json::from_str(&s)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Persist the config at mode 0600.
    pub fn save(&self) -> Result<()> {
        let dir = config_dir()?;
        fs::create_dir_all(&dir)?;
        let path = dir.join("config.json");
        fs::write(&path, serde_json::to_string_pretty(self)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    /// Resolve the active workspace.
    ///
    /// Precedence: `--workspace` flag > `SLACK_TOKEN` env (with optional
    /// `SLACK_COOKIE`) > `SLACK_WORKSPACE` env > configured default > sole workspace.
    pub fn resolve(&self, flag: Option<&str>) -> Result<(String, Workspace)> {
        if let Some(name) = flag {
            let p = self
                .workspaces
                .get(name)
                .ok_or_else(|| Error::Auth(format!("workspace not found: {name}")))?;
            return Ok((name.to_string(), p.clone()));
        }
        if let Ok(token) = std::env::var("SLACK_TOKEN") {
            let workspace = Workspace {
                token,
                cookie: std::env::var("SLACK_COOKIE").ok(),
                team: None,
                team_id: None,
                user: None,
                user_id: None,
                url: None,
                validated_at: None,
            };
            return Ok(("env".to_string(), workspace));
        }
        let name = std::env::var("SLACK_WORKSPACE")
            .ok()
            .or_else(|| self.default_workspace.clone())
            .or_else(|| {
                if self.workspaces.len() == 1 {
                    self.workspaces.keys().next().cloned()
                } else {
                    None
                }
            })
            .ok_or_else(|| Error::Auth("no workspace configured — run `slack auth add`".into()))?;
        let p = self
            .workspaces
            .get(&name)
            .ok_or_else(|| Error::Auth(format!("workspace not found: {name}")))?;
        Ok((name, p.clone()))
    }
}
