//! clap command and argument model.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "slack",
    version,
    about = "Operate Slack conversations from the command line"
)]
pub struct Args {
    /// Profile to use (overrides SLACK_PROFILE and the configured default)
    #[arg(long, global = true)]
    pub profile: Option<String>,

    /// Emit stable machine-readable JSON on stdout
    #[arg(long, global = true)]
    pub json: bool,

    /// Bypass the directory cache
    #[arg(long, global = true)]
    pub no_cache: bool,

    /// Log API calls to stderr
    #[arg(long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// Manage workspace profiles
    Auth {
        #[command(subcommand)]
        cmd: AuthCmd,
    },
    /// Channels
    Channel {
        #[command(subcommand)]
        cmd: ChannelCmd,
    },
    /// Messages
    Message {
        #[command(subcommand)]
        cmd: MessageCmd,
    },
    /// Search
    Search {
        #[command(subcommand)]
        cmd: SearchCmd,
    },
    /// Users
    User {
        #[command(subcommand)]
        cmd: UserCmd,
    },
    /// Reactions
    Reaction {
        #[command(subcommand)]
        cmd: ReactionCmd,
    },
    /// Files
    File {
        #[command(subcommand)]
        cmd: FileCmd,
    },
    /// Call any Web API method directly
    Api {
        /// Method name, e.g. conversations.members
        method: String,
        /// key=value form parameters
        params: Vec<String>,
        /// JSON body from a file, or '-' for stdin (instead of key=value)
        #[arg(long)]
        data: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum AuthCmd {
    /// Add a profile (prompts for the token; xoxc tokens also prompt for the d cookie)
    Add {
        /// Profile name
        name: String,
        /// Read token (line 1) and optional cookie (line 2) from stdin
        #[arg(long, conflicts_with = "curl")]
        token_stdin: bool,
        /// Paste a DevTools "Copy as cURL" command; extracts both token and cookie
        #[arg(long)]
        curl: bool,
    },
    /// List profiles
    List,
    /// Validate a profile against auth.test and report identity
    Status {
        /// Profile name (default: the active profile)
        name: Option<String>,
    },
    /// Set the default profile
    Use { name: String },
    /// Remove a profile
    Remove {
        name: String,
        /// Skip confirmation
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
pub enum ChannelCmd {
    /// List conversations
    List {
        /// Comma-separated: public_channel,private_channel,mpim,im
        #[arg(long, default_value = "public_channel,private_channel")]
        types: String,
        /// Include archived conversations
        #[arg(long)]
        archived: bool,
        /// Total result limit
        #[arg(long, default_value_t = 200, conflicts_with = "all")]
        limit: usize,
        /// Follow every cursor
        #[arg(long)]
        all: bool,
    },
    /// Show one conversation
    Info {
        /// Channel ID, #name, or name
        channel: String,
    },
}

#[derive(Subcommand)]
pub enum MessageCmd {
    /// List recent messages in a conversation (oldest first)
    List {
        /// Channel ID, #name, or @user (DM)
        channel: String,
        /// Only messages after this time (RFC 3339 or YYYY-MM-DD)
        #[arg(long)]
        after: Option<String>,
        /// Only messages before this time (RFC 3339 or YYYY-MM-DD)
        #[arg(long)]
        before: Option<String>,
        #[arg(long, default_value_t = 30, conflicts_with = "all")]
        limit: usize,
        #[arg(long)]
        all: bool,
    },
    /// Read a thread
    Thread {
        channel: String,
        /// Thread root timestamp
        timestamp: String,
        #[arg(long, default_value_t = 100, conflicts_with = "all")]
        limit: usize,
        #[arg(long)]
        all: bool,
    },
    /// Send a message to a channel, DM, or @user
    Send {
        /// Channel ID, #name, @user, or DM ID
        target: String,
        /// Message text (or --text-file, or stdin when piped)
        text: Vec<String>,
        /// Read message text from a file
        #[arg(long)]
        text_file: Option<String>,
        /// Reply in a thread (root timestamp)
        #[arg(long)]
        thread: Option<String>,
        /// Also send the thread reply to the channel
        #[arg(long, requires = "thread")]
        broadcast: bool,
        /// JSON blocks payload from a file
        #[arg(long)]
        blocks: Option<String>,
        /// Resolve and report without sending
        #[arg(long)]
        dry_run: bool,
    },
    /// Edit a message you own
    Edit {
        channel: String,
        timestamp: String,
        text: Vec<String>,
        #[arg(long)]
        text_file: Option<String>,
    },
    /// Delete a message you own
    Delete {
        channel: String,
        timestamp: String,
        /// Skip confirmation
        #[arg(long)]
        yes: bool,
    },
    /// Get a permalink to a message
    Permalink { channel: String, timestamp: String },
}

#[derive(Subcommand)]
pub enum SearchCmd {
    /// Search messages (requires a user or session token)
    Messages {
        query: Vec<String>,
        /// score | timestamp
        #[arg(long, default_value = "score")]
        sort: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
}

#[derive(Subcommand)]
pub enum UserCmd {
    /// List workspace members
    List {
        #[arg(long)]
        include_deleted: bool,
        #[arg(long, default_value_t = 200, conflicts_with = "all")]
        limit: usize,
        #[arg(long)]
        all: bool,
    },
    /// Show one user
    Info {
        /// User ID, @display-name, or name
        user: String,
    },
}

#[derive(Subcommand)]
pub enum FileCmd {
    /// Upload a file to a channel, DM, or thread
    Upload {
        /// Channel ID, #name, or @user
        channel: String,
        /// Path to the file
        path: String,
        /// File title shown in Slack (default: the filename)
        #[arg(long)]
        title: Option<String>,
        /// Message posted alongside the file
        #[arg(long)]
        comment: Option<String>,
        /// Upload into a thread (root timestamp)
        #[arg(long)]
        thread: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum ReactionCmd {
    /// Add a reaction
    Add {
        channel: String,
        timestamp: String,
        /// Emoji name, with or without colons
        emoji: String,
    },
    /// Remove a reaction
    Remove {
        channel: String,
        timestamp: String,
        emoji: String,
    },
}
