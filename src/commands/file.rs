//! File sharing via Slack's three-step external upload flow.
//!
//! `files.upload` was retired 2025-11-12. The replacement: reserve a
//! pre-signed URL per file, POST the bytes there, then complete the upload and
//! share into a conversation. `files.completeUploadExternal` accepts an
//! `initial_comment` and takes several files at once, so a message with
//! attachments is one operation — see `message send --attach`.

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
        } => {
            let dir = ctx.dir();
            let (conversation, label) = super::message::resolve_target(&dir, &channel)?;
            let paths = vec![path];
            let done = share(
                ctx,
                &conversation,
                &paths,
                title.as_deref(),
                comment.as_deref(),
                thread.as_deref(),
            )?;
            report(ctx, &done, &paths, &label)
        }
    }
}

/// Upload files and share them into a conversation, optionally with a comment.
///
/// Returns the raw `files.completeUploadExternal` response. `title` applies
/// only when sharing a single file; with several, each keeps its filename.
pub fn share(
    ctx: &Ctx,
    conversation: &str,
    paths: &[String],
    title: Option<&str>,
    comment: Option<&str>,
    thread: Option<&str>,
) -> Result<Value> {
    if paths.is_empty() {
        return Err(Error::Usage("no file to upload".into()));
    }
    let mut metas: Vec<Value> = Vec::with_capacity(paths.len());
    for path in paths {
        let (id, _) = upload_one(ctx, path)?;
        let mut meta = serde_json::json!({ "id": id });
        if let Some(t) = title
            && paths.len() == 1
        {
            meta["title"] = Value::String(t.to_string());
        }
        metas.push(meta);
    }

    let mut params = vec![
        ("files", Value::Array(metas).to_string()),
        ("channel_id", conversation.to_string()),
    ];
    if let Some(c) = comment {
        params.push(("initial_comment", c.to_string()));
    }
    if let Some(t) = thread {
        params.push(("thread_ts", t.to_string()));
    }
    ctx.client.call("files.completeUploadExternal", &params)
}

/// Reserve a URL, POST the bytes, and return the resulting (file id, filename).
fn upload_one(ctx: &Ctx, path: &str) -> Result<(String, String)> {
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

    let reserve = ctx.client.call(
        "files.getUploadURLExternal",
        &[
            ("filename", filename.clone()),
            ("length", bytes.len().to_string()),
        ],
    )?;
    let field = |k: &str| {
        reserve
            .get(k)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| Error::Api {
                method: "files.getUploadURLExternal".into(),
                code: format!("missing {k} in response"),
            })
    };
    let upload_url = field("upload_url")?;
    let file_id = field("file_id")?;
    ctx.client.put_bytes(&upload_url, bytes)?;
    Ok((file_id, filename))
}

/// Human/JSON summary of a completed share.
pub fn report(ctx: &Ctx, done: &Value, paths: &[String], label: &str) -> Result<()> {
    if ctx.json {
        print_json(done);
        return Ok(());
    }
    let names: Vec<&str> = paths
        .iter()
        .map(|p| {
            Path::new(p)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(p.as_str())
        })
        .collect();
    println!("uploaded {} to {label}", names.join(", "));
    if let Some(files) = done.get("files").and_then(Value::as_array) {
        for f in files {
            if let Some(link) = f.get("permalink").and_then(Value::as_str) {
                println!("{link}");
            }
        }
    }
    Ok(())
}
