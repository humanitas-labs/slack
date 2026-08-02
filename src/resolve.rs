//! Name → ID resolution with a per-workspace directory cache.
//!
//! Exact IDs always win and never hit the network. Names resolve against
//! `conversations.list` / `users.list`, cached under `~/.cache/slack/<workspace>/`
//! with a five-minute TTL. Ambiguity is an error with candidates, never a guess.

use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::error::{Error, Result};
use crate::slack::{Channel, Client, User};

const CACHE_TTL_SECS: u64 = 300;

/// How the operator addressed a target.
#[derive(Debug, Clone)]
pub enum Target {
    /// A literal Slack ID (C…, G…, D…, U…, W…).
    Id(String),
    /// `#name` or a bare name — a channel reference.
    Channel(String),
    /// `@name` — a user reference (DM target or user lookup).
    User(String),
}

/// Parse an operator-supplied target string.
pub fn parse_target(s: &str) -> Target {
    if let Some(name) = s.strip_prefix('@') {
        return Target::User(name.to_string());
    }
    if let Some(name) = s.strip_prefix('#') {
        return Target::Channel(name.to_string());
    }
    if looks_like_id(s) {
        return Target::Id(s.to_string());
    }
    Target::Channel(s.to_string())
}

/// Conservative Slack ID shape: known prefix, >= 9 chars, uppercase alphanumeric.
pub fn looks_like_id(s: &str) -> bool {
    let mut chars = s.chars();
    let first = chars.next();
    matches!(first, Some('C' | 'G' | 'D' | 'U' | 'W'))
        && s.len() >= 9
        && s.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

/// Lazily loaded, disk-cached workspace directory (channels + users).
pub struct Directory<'a> {
    client: &'a Client,
    partition: String,
    no_cache: bool,
    channels: RefCell<Option<Rc<Vec<Channel>>>>,
    users: RefCell<Option<Rc<Vec<User>>>>,
    /// True once the respective list was fetched live this invocation.
    channels_fresh: RefCell<bool>,
    users_fresh: RefCell<bool>,
}

impl<'a> Directory<'a> {
    pub fn new(client: &'a Client, partition: &str, no_cache: bool) -> Directory<'a> {
        Directory {
            client,
            partition: partition.to_string(),
            no_cache,
            channels: RefCell::new(None),
            users: RefCell::new(None),
            channels_fresh: RefCell::new(false),
            users_fresh: RefCell::new(false),
        }
    }

    fn cache_path(&self, name: &str) -> Option<PathBuf> {
        let dir = dirs::cache_dir()?.join("slack").join(&self.partition);
        Some(dir.join(format!("{name}.json")))
    }

    fn read_cache(&self, name: &str) -> Option<Vec<Value>> {
        if self.no_cache {
            return None;
        }
        let path = self.cache_path(name)?;
        let raw = fs::read_to_string(path).ok()?;
        let v: Value = serde_json::from_str(&raw).ok()?;
        let fetched = v.get("fetched").and_then(Value::as_u64)?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
        if now.saturating_sub(fetched) > CACHE_TTL_SECS {
            return None;
        }
        v.get("items").and_then(Value::as_array).cloned()
    }

    fn write_cache(&self, name: &str, items: &[Value]) {
        let Some(path) = self.cache_path(name) else {
            return;
        };
        let Some(parent) = path.parent() else {
            return;
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let doc = serde_json::json!({ "fetched": now, "items": items });
        let _ = fs::create_dir_all(parent);
        let _ = fs::write(path, doc.to_string());
    }

    /// All non-IM conversations (public, private, mpim), cached.
    pub fn channels(&self) -> Result<Rc<Vec<Channel>>> {
        if let Some(c) = self.channels.borrow().as_ref() {
            return Ok(Rc::clone(c));
        }
        let (raw, fresh) = match self.read_cache("channels") {
            Some(items) => (items, false),
            None => {
                let params = [
                    ("types", "public_channel,private_channel,mpim".to_string()),
                    ("exclude_archived", "false".to_string()),
                    ("limit", "1000".to_string()),
                ];
                let (items, _) =
                    self.client
                        .paged("conversations.list", &params, "channels", None)?;
                self.write_cache("channels", &items);
                (items, true)
            }
        };
        let parsed: Vec<Channel> = raw
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect();
        let rc = Rc::new(parsed);
        *self.channels.borrow_mut() = Some(Rc::clone(&rc));
        *self.channels_fresh.borrow_mut() = fresh;
        Ok(rc)
    }

    /// Full member directory, cached.
    pub fn users(&self) -> Result<Rc<Vec<User>>> {
        if let Some(u) = self.users.borrow().as_ref() {
            return Ok(Rc::clone(u));
        }
        let (raw, fresh) = match self.read_cache("users") {
            Some(items) => (items, false),
            None => {
                let params = [("limit", "500".to_string())];
                let (items, _) = self.client.paged("users.list", &params, "members", None)?;
                self.write_cache("users", &items);
                (items, true)
            }
        };
        let parsed: Vec<User> = raw
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect();
        let rc = Rc::new(parsed);
        *self.users.borrow_mut() = Some(Rc::clone(&rc));
        *self.users_fresh.borrow_mut() = fresh;
        Ok(rc)
    }

    fn invalidate_channels(&self) {
        *self.channels.borrow_mut() = None;
        if let Some(p) = self.cache_path("channels") {
            let _ = fs::remove_file(p);
        }
    }

    fn invalidate_users(&self) {
        *self.users.borrow_mut() = None;
        if let Some(p) = self.cache_path("users") {
            let _ = fs::remove_file(p);
        }
    }

    /// Resolve a channel reference to a conversation.
    ///
    /// IDs pass through via `conversations.info`; names match exact
    /// (case-insensitive) first, then substring. Ambiguity errors with
    /// candidates. A stale-cache miss retries once against the live list.
    pub fn resolve_channel(&self, query: &str) -> Result<Channel> {
        match parse_target(query) {
            Target::Id(id) => self.channel_info(&id),
            Target::User(name) => Err(Error::Usage(format!(
                "'@{name}' is a user reference; this argument takes a channel"
            ))),
            Target::Channel(name) => match self.match_channel(&name) {
                Ok(c) => Ok(c),
                Err(e @ Error::NotFound { .. }) => {
                    if *self.channels_fresh.borrow() {
                        return Err(e);
                    }
                    self.invalidate_channels();
                    self.match_channel(&name)
                }
                Err(e) => Err(e),
            },
        }
    }

    fn match_channel(&self, name: &str) -> Result<Channel> {
        let channels = self.channels()?;
        let needle = name.to_lowercase();
        let named: Vec<&Channel> = channels
            .iter()
            .filter(|c| {
                c.name
                    .as_deref()
                    .is_some_and(|n| n.to_lowercase() == needle)
            })
            .collect();
        let pool = if named.is_empty() {
            channels
                .iter()
                .filter(|c| {
                    c.name
                        .as_deref()
                        .is_some_and(|n| n.to_lowercase().contains(&needle))
                })
                .collect()
        } else {
            named
        };
        match pool.len() {
            0 => Err(Error::NotFound {
                kind: "channel",
                query: name.to_string(),
            }),
            1 => Ok(pool[0].clone()),
            _ => Err(Error::Ambiguous {
                kind: "channel",
                query: name.to_string(),
                candidates: pool
                    .iter()
                    .take(10)
                    .map(|c| format!("{} ({})", c.handle(), c.id))
                    .collect(),
            }),
        }
    }

    /// Fetch a single conversation by ID.
    pub fn channel_info(&self, id: &str) -> Result<Channel> {
        let v = self
            .client
            .call("conversations.info", &[("channel", id.to_string())])?;
        let c = v.get("channel").cloned().ok_or_else(|| Error::NotFound {
            kind: "channel",
            query: id.to_string(),
        })?;
        Ok(serde_json::from_value(c)?)
    }

    /// Resolve a user reference (display name, handle, real name, or ID).
    pub fn resolve_user(&self, query: &str) -> Result<User> {
        let name = query.strip_prefix('@').unwrap_or(query);
        if looks_like_id(name) && (name.starts_with('U') || name.starts_with('W')) {
            let v = self
                .client
                .call("users.info", &[("user", name.to_string())])?;
            let u = v.get("user").cloned().ok_or_else(|| Error::NotFound {
                kind: "user",
                query: name.to_string(),
            })?;
            return Ok(serde_json::from_value(u)?);
        }
        match self.match_user(name) {
            Ok(u) => Ok(u),
            Err(e @ Error::NotFound { .. }) => {
                if *self.users_fresh.borrow() {
                    return Err(e);
                }
                self.invalidate_users();
                self.match_user(name)
            }
            Err(e) => Err(e),
        }
    }

    fn match_user(&self, name: &str) -> Result<User> {
        let users = self.users()?;
        let needle = name.to_lowercase();
        let field_eq = |u: &User| {
            u.name.to_lowercase() == needle
                || u.profile.display_name.to_lowercase() == needle
                || u.real_name
                    .as_deref()
                    .is_some_and(|r| r.to_lowercase() == needle)
        };
        let field_contains = |u: &User| {
            u.name.to_lowercase().contains(&needle)
                || u.profile.display_name.to_lowercase().contains(&needle)
                || u.real_name
                    .as_deref()
                    .is_some_and(|r| r.to_lowercase().contains(&needle))
        };
        let alive = |u: &&User| !u.deleted;
        let exact: Vec<&User> = users.iter().filter(alive).filter(|u| field_eq(u)).collect();
        let pool = if exact.is_empty() {
            users
                .iter()
                .filter(alive)
                .filter(|u| field_contains(u))
                .collect()
        } else {
            exact
        };
        match pool.len() {
            0 => Err(Error::NotFound {
                kind: "user",
                query: name.to_string(),
            }),
            1 => Ok(pool[0].clone()),
            _ => Err(Error::Ambiguous {
                kind: "user",
                query: name.to_string(),
                candidates: pool
                    .iter()
                    .take(10)
                    .map(|u| format!("@{} ({})", u.label(), u.id))
                    .collect(),
            }),
        }
    }

    /// Display label for a user ID; falls back to the raw ID.
    pub fn user_label(&self, id: &str) -> String {
        if let Ok(users) = self.users()
            && let Some(u) = users.iter().find(|u| u.id == id)
        {
            return u.label().to_string();
        }
        id.to_string()
    }

    /// Open (or fetch) the DM conversation with a user.
    pub fn dm_with(&self, user_id: &str) -> Result<String> {
        let v = self
            .client
            .call("conversations.open", &[("users", user_id.to_string())])?;
        v.pointer("/channel/id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| Error::NotFound {
                kind: "dm",
                query: user_id.to_string(),
            })
    }
}
