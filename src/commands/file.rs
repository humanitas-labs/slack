//! File upload via Slack's three-step external flow.
//!
//! `files.upload` was retired 2025-11-12. The replacement: ask for a
//! pre-signed URL, PUT the bytes there, then complete the upload and share it
//! into a conversation. Each step can fail independently, so each reports
//! which stage broke.

use std::path::Path;

use serde_json::Value;

use crate::cli::FileCmd;
use crate::error::{Error, Result};
use crate::output::print_json;

use super::Ctx;

pub fn run(ctx: &Ctx, cmd: FileCmd) -> Result<()> {
    match cmd {
        FileCmd::Upload {
            channel,
            path,
            title,
            comment,
            thread,
        } => upload(
            ctx,
            &channel,
            &path,
            title.as_deref(),
            comment.as_deref(),
            thread.as_deref(),
        ),
    }
}

fn upload(
    ctx: &Ctx,
    channel: &str,
    path: &str,
    title: Option<&str>,
    comment: Option<&str>,
    thread: Option<&str>,
) -> Result<()> {
    let file = Path::new(path);
    let bytes =
        std::fs::read(file).map_err(|e| Error::Usage(format!("cannot read '{path}': {e}")))?;
    if bytes.is_empty() {
        return Err(Error::Usage(format!("'{path}' is empty")));
    }
    let filename = file
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| Error::Usage(format!("cannot determine a filename from '{path}'")))?
        .to_string();

    let dir = ctx.dir();
    let (conversation, label) = super::message::resolve_target(&dir, channel)?;

    // Step 1 — reserve an upload URL.
    let reserve = ctx.client.call(
        "files.getUploadURLExternal",
        &[
            ("filename", filename.clone()),
            ("length", bytes.len().to_string()),
        ],
    )?;
    let upload_url = reserve
        .get("upload_url")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Api {
            method: "files.getUploadURLExternal".into(),
            code: "missing upload_url in response".into(),
        })?;
    let file_id = reserve
        .get("file_id")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Api {
            method: "files.getUploadURLExternal".into(),
            code: "missing file_id in response".into(),
        })?
        .to_string();

    // Step 2 — PUT the bytes to the pre-signed URL (no Slack auth here).
    ctx.client.put_bytes(upload_url, bytes)?;

    // Step 3 — complete and share into the conversation.
    let mut files_meta = serde_json::json!({ "id": file_id });
    if let Some(t) = title {
        files_meta["title"] = Value::String(t.to_string());
    }
    let mut params = vec![
        ("files", Value::Array(vec![files_meta]).to_string()),
        ("channel_id", conversation),
    ];
    if let Some(c) = comment {
        params.push(("initial_comment", c.to_string()));
    }
    if let Some(t) = thread {
        params.push(("thread_ts", t.to_string()));
    }
    let done = ctx.client.call("files.completeUploadExternal", &params)?;

    if ctx.json {
        print_json(&done);
        return Ok(());
    }
    let permalink = done
        .pointer("/files/0/permalink")
        .and_then(Value::as_str)
        .unwrap_or("");
    println!("uploaded {filename} to {label}");
    if !permalink.is_empty() {
        println!("{permalink}");
    }
    Ok(())
}
