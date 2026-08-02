//! Message commands: list, thread, send, edit, delete, permalink.

use std::io::{IsTerminal, Read};

use serde_json::Value;

use crate::cli::MessageCmd;
use crate::error::{Error, Result};
use crate::output::{clean_text, parse_time, print_json, ts_local};
use crate::resolve::{Directory, Target, parse_target};
use crate::slack::Message;

use super::Ctx;

pub fn run(ctx: &Ctx, cmd: MessageCmd) -> Result<()> {
    match cmd {
        MessageCmd::List {
            channel,
            after,
            before,
            limit,
            all,
        } => list(
            ctx,
            &channel,
            after.as_deref(),
            before.as_deref(),
            if all { None } else { Some(limit) },
        ),
        MessageCmd::Thread {
            channel,
            timestamp,
            limit,
            all,
        } => thread(
            ctx,
            &channel,
            &timestamp,
            if all { None } else { Some(limit) },
        ),
        MessageCmd::Send {
            target,
            text,
            text_file,
            thread,
            broadcast,
            attach,
            blocks,
            dry_run,
        } => send(
            ctx,
            &target,
            &text,
            text_file.as_deref(),
            thread.as_deref(),
            broadcast,
            &attach,
            blocks.as_deref(),
            dry_run,
        ),
        MessageCmd::Edit {
            channel,
            timestamp,
            text,
            text_file,
        } => edit(ctx, &channel, &timestamp, &text, text_file.as_deref()),
        MessageCmd::Delete {
            channel,
            timestamp,
            yes,
        } => delete(ctx, &channel, &timestamp, yes),
        MessageCmd::Permalink { channel, timestamp } => permalink(ctx, &channel, &timestamp),
    }
}

/// Resolve a message target: channel name/ID, or `@user` → DM conversation.
/// Returns (conversation id, human label).
pub fn resolve_target(dir: &Directory, target: &str) -> Result<(String, String)> {
    match parse_target(target) {
        Target::User(name) => {
            let user = dir.resolve_user(&name)?;
            let dm = dir.dm_with(&user.id)?;
            Ok((dm, format!("@{}", user.label())))
        }
        Target::Id(id) if id.starts_with('U') || id.starts_with('W') => {
            let dm = dir.dm_with(&id)?;
            Ok((dm, format!("@{}", dir.user_label(&id))))
        }
        Target::Id(id) => Ok((id.clone(), id)),
        Target::Channel(_) => {
            let c = dir.resolve_channel(target)?;
            Ok((c.id.clone(), c.handle()))
        }
    }
}

fn sender_label(m: &Message, dir: &Directory) -> String {
    if let Some(u) = &m.user {
        dir.user_label(u)
    } else if let Some(n) = &m.username {
        n.clone()
    } else if let Some(b) = &m.bot_id {
        b.clone()
    } else {
        "?".to_string()
    }
}

/// Body text, with a placeholder when the message carries only files or
/// attachments and would otherwise render blank.
fn body_text(m: &Message, dir: &Directory) -> String {
    let text = clean_text(&m.text, dir);
    if !text.is_empty() {
        return text;
    }
    if !m.files.is_empty() {
        return m
            .files
            .iter()
            .map(|f| match &f.id {
                Some(id) => format!("[file {id}: {}]", f.label()),
                None => format!("[file: {}]", f.label()),
            })
            .collect::<Vec<_>>()
            .join(" ");
    }
    if !m.attachments.is_empty() {
        return "[attachment]".to_string();
    }
    text
}

fn print_messages(msgs: &[Message], dir: &Directory, show_thread_markers: bool) {
    for m in msgs {
        let head = format!("[{}] {}", ts_local(&m.ts), sender_label(m, dir));
        println!("{head}: {}", body_text(m, dir));
        if show_thread_markers
            && let Some(n) = m.reply_count
            && n > 0
        {
            println!("    ({} replies in thread — ts {})", n, m.ts);
        }
    }
}

fn list(
    ctx: &Ctx,
    channel: &str,
    after: Option<&str>,
    before: Option<&str>,
    limit: Option<usize>,
) -> Result<()> {
    let dir = ctx.dir();
    let (id, label) = resolve_target(&dir, channel)?;
    let mut params = vec![("channel", id), ("limit", "200".to_string())];
    if let Some(t) = after {
        let ts = parse_time(t).ok_or_else(|| {
            Error::Usage(format!("cannot parse time '{t}' (RFC 3339 or YYYY-MM-DD)"))
        })?;
        params.push(("oldest", ts));
    }
    if let Some(t) = before {
        let ts = parse_time(t).ok_or_else(|| {
            Error::Usage(format!("cannot parse time '{t}' (RFC 3339 or YYYY-MM-DD)"))
        })?;
        params.push(("latest", ts));
    }
    let (items, next) = ctx
        .client
        .paged("conversations.history", &params, "messages", limit)?;
    if ctx.json {
        print_json(
            &serde_json::json!({ "channel": label, "messages": items, "next_cursor": next }),
        );
        return Ok(());
    }
    let mut msgs: Vec<Message> = items
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();
    msgs.reverse(); // history returns newest-first; read oldest-first
    println!("-- {label} --");
    print_messages(&msgs, &dir, true);
    if next.is_some() {
        eprintln!("more history available — raise --limit or pass --all");
    }
    Ok(())
}

fn thread(ctx: &Ctx, channel: &str, ts: &str, limit: Option<usize>) -> Result<()> {
    let dir = ctx.dir();
    let (id, label) = resolve_target(&dir, channel)?;
    let params = vec![
        ("channel", id),
        ("ts", ts.to_string()),
        ("limit", "200".to_string()),
    ];
    let (items, next) = ctx
        .client
        .paged("conversations.replies", &params, "messages", limit)?;
    if ctx.json {
        print_json(
            &serde_json::json!({ "channel": label, "messages": items, "next_cursor": next }),
        );
        return Ok(());
    }
    let msgs: Vec<Message> = items
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();
    println!("-- thread in {label} --");
    print_messages(&msgs, &dir, false);
    if next.is_some() {
        eprintln!("more replies available — raise --limit or pass --all");
    }
    Ok(())
}

/// Gather message text from exactly one source: positional args, --text-file,
/// or piped stdin.
fn gather_text(positional: &[String], text_file: Option<&str>) -> Result<String> {
    let from_args = if positional.is_empty() {
        None
    } else {
        Some(positional.join(" "))
    };
    let from_file = match text_file {
        Some(path) => Some(std::fs::read_to_string(path)?),
        None => None,
    };
    match (from_args, from_file) {
        (Some(_), Some(_)) => Err(Error::Usage(
            "provide message text positionally or via --text-file, not both".into(),
        )),
        (Some(t), None) | (None, Some(t)) => Ok(t.trim_end().to_string()),
        (None, None) => {
            if std::io::stdin().is_terminal() {
                return Err(Error::Usage(
                    "no message text: pass it positionally, via --text-file, or pipe stdin".into(),
                ));
            }
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            let buf = buf.trim_end().to_string();
            if buf.is_empty() {
                return Err(Error::Usage("stdin was empty".into()));
            }
            Ok(buf)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn send(
    ctx: &Ctx,
    target: &str,
    text: &[String],
    text_file: Option<&str>,
    thread: Option<&str>,
    broadcast: bool,
    attach: &[String],
    blocks: Option<&str>,
    dry_run: bool,
) -> Result<()> {
    // Attachments and blocks may travel without body text; everything else
    // requires it.
    let text_optional = blocks.is_some() || !attach.is_empty();
    let body = if text_optional && text.is_empty() && text_file.is_none() {
        String::new()
    } else {
        gather_text(text, text_file)?
    };
    let dir = ctx.dir();
    let (id, label) = resolve_target(&dir, target)?;

    let identity = format!(
        "{} ({}) [{} token]",
        ctx.workspace.user.as_deref().unwrap_or("?"),
        ctx.workspace.user_id.as_deref().unwrap_or("?"),
        ctx.workspace.kind().label(),
    );
    if dry_run {
        println!("would send as {identity}");
        println!(
            "to {label} ({id}){}",
            match thread {
                Some(t) => format!(", thread {t}"),
                None => String::new(),
            }
        );
        if !attach.is_empty() {
            for path in attach {
                let size = std::fs::metadata(path)
                    .map(|m| format!("{} bytes", m.len()))
                    .unwrap_or_else(|e| format!("UNREADABLE: {e}"));
                println!("attach: {path} ({size})");
            }
        }
        println!("---\n{body}");
        return Ok(());
    }

    // With attachments the share call carries the text as the file's comment,
    // so this is one Slack operation, not a send followed by an upload.
    if !attach.is_empty() {
        let comment = if body.is_empty() {
            None
        } else {
            Some(body.as_str())
        };
        let done = super::file::share(ctx, &id, attach, None, comment, thread)?;
        return super::file::report(ctx, &done, attach, &label);
    }

    let mut params = vec![("channel", id.clone())];
    if !body.is_empty() {
        params.push(("text", body));
    }
    if let Some(t) = thread {
        params.push(("thread_ts", t.to_string()));
        if broadcast {
            params.push(("reply_broadcast", "true".to_string()));
        }
    }
    if let Some(path) = blocks {
        let raw = std::fs::read_to_string(path)?;
        let parsed: Value = serde_json::from_str(&raw)?; // validate before sending
        params.push(("blocks", parsed.to_string()));
    }
    let v = ctx.client.call("chat.postMessage", &params)?;

    // Identity contract: an operator token must post as the stored member.
    if ctx.workspace.kind().acts_as_member() {
        let author = v.pointer("/message/user").and_then(Value::as_str);
        let expected = ctx.workspace.user_id.as_deref();
        if let (Some(a), Some(e)) = (author, expected)
            && a != e
        {
            return Err(Error::Auth(format!(
                "identity mismatch: message posted as {a}, expected {e}"
            )));
        }
    }

    if ctx.json {
        print_json(&v);
        return Ok(());
    }
    let ts = v.get("ts").and_then(Value::as_str).unwrap_or("?");
    println!("sent to {label} as {identity} (ts {ts})");
    Ok(())
}

fn edit(
    ctx: &Ctx,
    channel: &str,
    ts: &str,
    text: &[String],
    text_file: Option<&str>,
) -> Result<()> {
    let body = gather_text(text, text_file)?;
    let dir = ctx.dir();
    let (id, label) = resolve_target(&dir, channel)?;
    let v = ctx.client.call(
        "chat.update",
        &[("channel", id), ("ts", ts.to_string()), ("text", body)],
    )?;
    if ctx.json {
        print_json(&v);
        return Ok(());
    }
    println!("edited {label} ts {ts}");
    Ok(())
}

fn delete(ctx: &Ctx, channel: &str, ts: &str, yes: bool) -> Result<()> {
    let dir = ctx.dir();
    let (id, label) = resolve_target(&dir, channel)?;
    super::confirm(&format!("delete message {ts} in {label}?"), yes)?;
    let v = ctx
        .client
        .call("chat.delete", &[("channel", id), ("ts", ts.to_string())])?;
    if ctx.json {
        print_json(&v);
        return Ok(());
    }
    println!("deleted {label} ts {ts}");
    Ok(())
}

fn permalink(ctx: &Ctx, channel: &str, ts: &str) -> Result<()> {
    let dir = ctx.dir();
    let (id, _) = resolve_target(&dir, channel)?;
    let v = ctx.client.call(
        "chat.getPermalink",
        &[("channel", id), ("message_ts", ts.to_string())],
    )?;
    if ctx.json {
        print_json(&v);
        return Ok(());
    }
    println!(
        "{}",
        v.get("permalink").and_then(Value::as_str).unwrap_or("?")
    );
    Ok(())
}
