//! Wire types for the subset of the Web API the CLI renders.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Channel {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub is_im: bool,
    #[serde(default)]
    pub is_mpim: bool,
    #[serde(default)]
    pub is_private: bool,
    #[serde(default)]
    pub is_archived: bool,
    #[serde(default)]
    pub is_member: bool,
    /// DM partner user id (`is_im` conversations only).
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub num_members: Option<u64>,
}

impl Channel {
    /// Human handle: `#name` for channels, the id for DMs (caller resolves).
    pub fn handle(&self) -> String {
        match &self.name {
            Some(n) if !n.is_empty() => format!("#{n}"),
            _ => self.id.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UserProfile {
    #[serde(default)]
    pub display_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct User {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub real_name: Option<String>,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub is_bot: bool,
    #[serde(default)]
    pub profile: UserProfile,
}

impl User {
    /// Best display label: display name, else real name, else handle.
    pub fn label(&self) -> &str {
        if !self.profile.display_name.is_empty() {
            &self.profile.display_name
        } else if let Some(r) = &self.real_name {
            if r.is_empty() { &self.name } else { r }
        } else {
            &self.name
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Message {
    #[serde(default)]
    pub user: Option<String>,
    /// Present on some bot/app messages.
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub bot_id: Option<String>,
    #[serde(default)]
    pub text: String,
    pub ts: String,
    #[serde(default)]
    pub reply_count: Option<u64>,
    #[serde(default)]
    pub files: Vec<MessageFile>,
    #[serde(default)]
    pub attachments: Vec<serde_json::Value>,
}

/// Minimal view of a file shared in a message.
#[derive(Debug, Clone, Deserialize)]
pub struct MessageFile {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
}

impl MessageFile {
    pub fn label(&self) -> &str {
        match (&self.name, &self.title) {
            (Some(n), _) if !n.is_empty() => n,
            (_, Some(t)) if !t.is_empty() => t,
            _ => "file",
        }
    }
}
