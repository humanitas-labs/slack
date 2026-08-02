---
title: "Render Slack conversations to a local folder"
date: 2026-08-02
status: idea
affects: "Future command surface"
---

# Render Slack conversations to a local folder

> Add `slack render` as an incremental, filesystem-native projection of selected Slack conversations. Do not make it an unrestricted workspace dump or describe it as a compliance archive.

## 1. Purpose

`slack render` would materialize Slack conversations as deterministic Markdown. The result would work with `rg`, Zed, Codex, backups, and ordinary filesystem tools without requiring a live query for every read.

This is distinct from Slack MCP. MCP is appropriate for targeted live retrieval by an AI client. Rendering is appropriate for repeated local analysis, cross-channel search, offline reading, and durable links between Slack and local project files.

## 2. Proposed command surface

```text
slack render <output> --channel <ref>... [--since <time>]
slack render <output> --all-channels [--since <time>]
slack render <output> --update

Options:
  --channel <ref>...       Channel ID or #name; repeatable
  --all-channels           Render every accessible public/private channel
  --since <time>           RFC 3339, YYYY-MM-DD, or relative duration such as 90d
  --include-dms            Include direct and multi-party messages
  --download-files         Download file contents instead of linking metadata only
  --reconcile-days <n>     Re-fetch recent history to detect edits; default 7
  --prune                  Remove local material no longer present or accessible
  --allow-vcs              Permit output inside a version-controlled tree
  --config <path>          Read the render selection and policy from TOML
```

Examples:

```sh
slack render ./workspace-slack \
  --channel '#product' \
  --channel '#engineering' \
  --since 90d

slack render ./workspace-slack --update
slack render ./workspace-slack --all-channels --since 30d
slack render ./workspace-slack --include-dms
```

Configuration form:

```toml
output = "./workspace-slack"
since = "90d"
channels = ["#product", "#engineering"]
include_dms = false
download_files = false
reconcile_days = 7
```

## 3. Output model

Shard transcripts by month. One file per channel will become slow to rewrite and unpleasant to review.

```text
workspace-slack/
├── index.md
├── channels/
│   ├── product--C012345/
│   │   ├── index.md
│   │   ├── 2026-07.md
│   │   └── 2026-08.md
│   └── engineering--C067890/
│       ├── index.md
│       └── 2026-08.md
├── users.json
├── .gitignore
└── .slack-render.json
```

Channel `index.md` should contain the durable channel ID, name, topic, purpose, membership state, render range, and links to monthly transcript shards. The workspace index should link every rendered conversation and report the last successful update.

Transcript shape:

```md
## 2026.08.02

### 14:32 · Andrew Jones

Deployment is complete.

- Thread reply, 14:35 · Ada Lovelace  
  Confirmed in production.

Source: [Open in Slack](https://example.slack.com/archives/C012345/p1785706320000100)
Slack timestamp: `1785706320.000100`
```

Preserve Slack timestamps, channel IDs, user IDs, edit timestamps, and permalinks as source metadata. Resolve mentions for readability. Embed thread replies below their root message. Link file metadata by default; downloading file contents requires `--download-files`.

## 4. Synchronization model

`.slack-render.json` is machine-owned state. It records the workspace ID, selected channels, per-channel high-water marks, rendered time range, schema version, and content hashes required for deterministic updates.

An update should:

1. Resolve the configured channel selection.
2. Fetch messages after the stored high-water mark.
3. Re-fetch the previous `reconcile_days` window to capture edits and recent deletions.
4. Fetch thread replies for roots that are new or changed in that window.
5. Rewrite only affected monthly shards.
6. Update indexes and state atomically after all channel writes succeed.

Slack returns cursored history and thread results. Rendering must use the shared paginator and honor `Retry-After`; it must not depend on fixed rate-tier capacities [1](https://docs.slack.dev/apis/web-api/rate-limits/).

`render` is a current local projection, not an immutable archive. It should not delete local data implicitly. `--prune` performs destructive reconciliation after displaying what will be removed. Historical edits or deletions outside the reconciliation window may remain until a wider refresh is requested.

## 5. Safety defaults

- Require at least one `--channel`, `--all-channels`, or a configuration file.
- Exclude DMs and group DMs unless `--include-dms` is explicit.
- Refuse to render into a version-controlled tree unless `--allow-vcs` is explicit.
- Create the output directory with mode `0700` and transcript/state files with mode `0600` on Unix.
- Create an output-local `.gitignore` that ignores all rendered contents by default.
- Never print message bodies or signed file URLs in verbose logs.
- Use the supported user-token path for bulk rendering. Do not use browser-session credentials for a workspace-wide historical collection.
- Keep downloaded files disabled by default. Preserve Slack links and metadata instead.
- Resolve ambiguous channel references as errors. Never guess.
- Make every shard deterministic so an unchanged update produces no file diff.

## 6. Scope boundaries

Initial scope:

- public and private channels visible to the selected user profile;
- bounded history and incremental updates;
- messages, edits visible in fetched responses, threads, reactions, file metadata, and permalinks;
- Markdown transcripts plus machine state.

Deferred:

- Events API or Socket Mode for real-time synchronization;
- full file download and attachment deduplication;
- search indexes beyond filesystem text search;
- HTML, PDF, or static-site output;
- retention-policy enforcement or legal hold behavior;
- Enterprise export, Audit Logs, and Discovery APIs;
- claims of archival completeness.

## 7. Promotion criteria

Promote this idea into an implementation plan only after:

1. The current P1 review findings in authentication, identity enforcement, and file upload are resolved.
2. The shared paginator returns correct results at boundaries smaller than a Slack response page.
3. A mockable API base URL and transport test harness cover 200/`ok:false`, 429, malformed responses, pagination, and timeouts.
4. A small spike renders one channel with a root message, edited message, thread, reaction, and file link into deterministic Markdown.
5. A second run with no Slack changes produces no filesystem changes.
6. The supported user-token profile can render the selected channel without using the browser-session path.

The first implementation should target one selected channel and one bounded date range. Add `--all-channels`, DMs, downloads, and pruning only after the incremental model is proven.
