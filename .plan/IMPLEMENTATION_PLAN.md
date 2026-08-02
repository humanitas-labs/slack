---
title: "Slack operator CLI implementation plan"
date: 2026-08-02
status: draft
affects: "Complete CLI"
---

# Slack operator CLI implementation plan

> Build this as a Rust CLI for people and scripts operating an existing Slack workspace. Use `slack` as the executable, named token profiles, a small typed command surface, and a raw Web API escape hatch. Do not build app-development or event-listener features.

## Table of Contents

1. [Context and recommendation](#1-context-and-recommendation)
2. [Product boundary](#2-product-boundary)
3. [Command surface](#3-command-surface)
4. [Authentication and scopes](#4-authentication-and-scopes)
5. [Architecture](#5-architecture)
6. [Implementation sequence](#6-implementation-sequence)
7. [Files touched](#7-files-touched)
8. [Verification](#8-verification)
9. [Risks and decisions](#9-risks-and-decisions)
10. [Definition of done](#10-definition-of-done)

---

## 1. Context and recommendation

The repository has no implementation to preserve. Treat the existing `Cargo.toml` as disposable scaffolding. Replace it from first principles; no dependency or runtime choice survives merely because it is already listed.

Keep Rust. Go is the strongest alternative because it also produces a single binary and has a mature CLI ecosystem. Python and TypeScript would shorten some API code but impose a runtime or bundling burden on every installation. Rust earns the choice through explicit domain types, controlled error handling, secret-safe abstractions, and a self-contained binary. This tool is request/response bound, not concurrency bound, so use a synchronous client. Introduce async only if the product later gains event streaming, Socket Mode, or concurrent bulk operations.

Call the executable `slack`. Slack publishes an official app-development CLI under the same name [1](https://docs.slack.dev/tools/slack-cli/), but this is a manageable `PATH` collision on machines under our control. If both tools are required, install or alias Slack's official binary as `slack-dev`. Use:

- repository: `slack`
- Cargo package: `slack-cli`
- executable: `slack`
- description: “Operate Slack conversations from the command line”

The first release should serve a single operator or internal team. Its default mode is a user OAuth token issued to the operator, so messages are posted as that Slack member rather than as a bot. Slack still requires an app to issue the token. Create one private internal app as the authorization container, grant user scopes, install it as the operator, and import the resulting token. The app needs no server, Events API, Socket Mode, slash commands, or public distribution.

A user token can act with the installing user’s access, while a bot token acts as the installed app and normally sees only conversations available to that bot [2](https://docs.slack.dev/authentication/tokens/). User identity is therefore a product requirement, not an optional token mode. The first capability test must prove that `chat.postMessage` attributes the message to the same user ID returned by `auth.test`. Bot tokens remain supported for explicitly bot-owned automation.

---

## 2. Product boundary

### 2.1 — Primary use cases

The CLI should make these workflows fast and scriptable:

1. Inspect channels, recent messages, and threads.
2. Send messages attributed to the authenticated Slack user, then edit, delete, and link to those messages.
3. Search messages when the selected token supports it.
4. Resolve human channel and user references to Slack IDs.
5. Add or remove reactions.
6. Upload a file to a channel or thread.
7. Call an unsupported Slack Web API method without waiting for a first-class command.

### 2.2 — Explicit exclusions from v0.1

- Creating, deploying, or running Slack apps. Slack’s official CLI owns this surface.
- Events API, Socket Mode, RTM, interactive components, or a resident daemon.
- Admin, Enterprise Grid, audit-log, workflow, canvas, list, and SCIM commands.
- Incoming webhook management.
- OAuth browser login and token refresh. Add these only if the CLI will be distributed beyond an internal operator group.
- Bulk workspace export or archival.
- An SDK-quality public Rust API. The crate is an application.

### 2.3 — UX contract

- IDs always work. Accept `#channel-name` and `@display-name` where resolution is unambiguous. Return candidate IDs on ambiguity.
- Human-readable output is the default. `--json` emits stable machine-readable output with no color or commentary. The raw `api` command returns Slack’s JSON response.
- Data goes to stdout. Diagnostics, warnings, and retry notices go to stderr.
- List commands are bounded by default. `--limit` is a total result limit; `--all` explicitly follows every cursor.
- Message text may be positional, read from `--text-file`, or read from stdin when stdin is not a terminal. Reject competing input sources.
- Destructive commands prompt on an interactive terminal and require `--yes` in non-interactive use.
- No command accepts a token as a normal command-line argument. Tokens must not enter shell history or process listings.
- Exit codes are stable: `0` success, `2` usage, `3` authentication/configuration, `4` Slack API rejection, `5` transport/protocol failure, and `6` rate limit not retried.

---

## 3. Command surface

### 3.1 — Global form

```text
slack [--profile NAME] [--json] [--no-color] [--verbose] <command>
```

Configuration precedence is explicit flags, environment, selected profile, then the default profile. `SLACK_TOKEN` is a process-local token override for CI and one-off use. `SLACK_PROFILE` selects a profile. `NO_COLOR` is honored.

### 3.2 — v0.1 commands

```text
slack auth add <name> [--token-stdin]       # prompt securely by default, validate with auth.test
slack auth list
slack auth status [<name>]
slack auth use <name>
slack auth remove <name> [--yes]

slack channel list [--types TYPES] [--archived] [--limit N | --all]
slack channel info <channel>

slack message list <channel> [--after TIME] [--before TIME] [--limit N | --all]
slack message thread <channel> <timestamp> [--limit N | --all]
slack message send <target> [text] [--text-file PATH] [--thread TS] [--broadcast]
                   [--blocks PATH] [--dry-run]
slack message edit <channel> <timestamp> [text] [--text-file PATH] [--blocks PATH]
slack message delete <channel> <timestamp> [--yes]
slack message permalink <channel> <timestamp>

slack search messages <query...> [--sort score|timestamp] [--limit N | --all]

slack user list [--include-deleted] [--limit N | --all]
slack user info <user>

slack reaction add <channel> <timestamp> <emoji>
slack reaction remove <channel> <timestamp> <emoji>

slack file upload <channel> <path> [--title TEXT] [--comment TEXT] [--thread TS]

slack api <method> [key=value ...]
slack api <method> --data <PATH|->
```

Examples:

```text
slack --profile acme channel list --types public_channel,private_channel
slack message list '#general' --after 2026-08-01T00:00:00Z --limit 50
printf 'deployment complete' | slack message send '#ops'
slack message send '@Ada Lovelace' 'Can you review this?' --dry-run
slack search messages 'from:andrew in:shipping regression'
slack reaction add C01234567 1785700800.000100 white_check_mark
slack api conversations.members channel=C01234567 limit=200
```

### 3.3 — Surface rules

Use singular resource nouns (`channel`, `message`, `user`, `reaction`, `file`) followed by verbs. Keep Slack’s native timestamp as the durable message identifier; do not invent a local message ID. Parse RFC 3339 and `YYYY-MM-DD` for time filters, then translate them to Slack timestamps.

`message send` resolves a channel, DM ID, user ID, `#channel-name`, or `@display-name`. Usernames are deprecated as addressing primitives, so `@display-name` is local resolution against `users.list`, not a Slack username sent directly to `chat.postMessage` [3](https://docs.slack.dev/reference/methods/chat.postMessage/).

The `api` command is the pressure-release valve. It sends a JSON POST to `https://slack.com/api/<method>`, which the Web API supports, and prints the unmodified response. It prevents demand for an exhaustive wrapper while real usage reveals which methods deserve typed commands. It must still apply authentication, timeouts, redaction, rate-limit handling, and Slack `ok: false` error semantics.

### 3.4 — Deferred commands

Add a command only after observed use through `slack api` or a concrete workflow requires it. Likely v0.2 candidates are:

```text
slack channel members <channel>
slack conversation open <user...>
slack message schedule <target> <time> [text]
slack bookmark list|add|edit|remove <channel> ...
slack pin add|remove <channel> <timestamp>
slack file info|download <file>
```

---

## 4. Authentication and scopes

### 4.1 — Profile model

Slack does not provide a supported personal API login independent of an app. “Authenticate as myself” means authorizing a private internal Slack app to receive a user token (`xoxp-`) for the installing member. The app is a permission and token container; it does not need to run any code.

`auth add` reads a user (`xoxp-`) or bot (`xoxb-`) token from a hidden prompt or stdin, calls `auth.test`, and stores:

- the token in the operating system credential store through the `keyring` crate;
- non-secret metadata in the platform configuration directory as TOML;
- profile name, team ID, team name, user ID, token kind, and last validation time.

Use the keychain service name `slack-cli` and a key derived from the profile name and team ID. This avoids sharing credential records with Slack's official CLI even when both executables use the name `slack`. Do not provide a plaintext token fallback. Headless systems use `SLACK_TOKEN`; CI should inject that variable from its own secret store.

`auth status` calls `auth.test` and reports the posting identity as either `user <name> (<user_id>)` or `bot <name> (<bot_id>)`. `message send --dry-run` reports the same identity before showing the resolved target. Neither command prints the token. Errors and verbose logs must redact the `Authorization` header and strings matching known Slack token prefixes.

The default documented profile is a user token. When `message send` uses a user profile, the returned message author must equal the authenticated user ID recorded by `auth.test`; a mismatch is an identity error rather than a successful send. A bot profile may send only when the operator deliberately selects that profile, and the human-readable result must identify the bot as the author.

OAuth is deferred because it changes the product from a local internal tool into a distributable Slack app. If that requirement appears, use OAuth v2, a localhost callback, PKCE where Slack supports it, state validation, and token rotation. Slack’s OAuth endpoint may return bot and user tokens, and rotated tokens carry refresh tokens and expiry metadata [4](https://docs.slack.dev/reference/methods/oauth.v2.access/).

### 4.2 — Scope matrix

| CLI capability | Slack method(s) | Required scopes and constraints |
|---|---|---|
| Validate profile | `auth.test` | Valid token |
| List/resolve channels | `conversations.list`, `conversations.info` | Relevant `channels:read`, `groups:read`, `im:read`, `mpim:read` scopes [5](https://docs.slack.dev/reference/methods/conversations.list/) |
| Read messages/threads | `conversations.history`, `conversations.replies` | Relevant `*:history` scopes; visibility differs for bot and user tokens [6](https://docs.slack.dev/reference/methods/conversations.history/) |
| Send/edit/delete/link as the operator | `chat.postMessage`, `chat.update`, `chat.delete`, `chat.getPermalink` | User token with `chat:write`; returned author must match the `auth.test` user ID; caller may only edit or delete messages it owns |
| Resolve users | `users.list`, `users.info` | `users:read`; do not request `users:read.email` in v0.1 |
| Search | `search.messages` | User token and `search:read`; this is a legacy scope, so treat search as an optional capability [7](https://docs.slack.dev/reference/scopes/search.read/) |
| Reactions | `reactions.add`, `reactions.remove` | `reactions:write` [8](https://docs.slack.dev/reference/methods/reactions.add/) |
| File upload | `files.getUploadURLExternal`, upload URL, `files.completeUploadExternal` | `files:write` |
| Raw API | User-selected | Depends on method |

Ship an `examples/app-manifest.yaml` whose primary configuration requests the least-privilege user scopes needed by this CLI. Document the one-time flow: create the private app from the manifest, install it to the workspace as the operator, obtain the user token, and run `slack auth add`. Include bot scopes as a separate optional configuration for bot-owned automation. Do not request every scope by default.

Slack retired `files.upload` on 2025-11-12. File upload must implement the current three-step external upload flow rather than the removed endpoint [9](https://docs.slack.dev/changelog/2024-04-a-better-way-to-upload-files-is-here-to-stay/).

---

## 5. Architecture

### 5.1 — Module boundaries

```text
main
  -> cli parser
  -> app orchestration
      -> profile/config + credential store
      -> resolvers
      -> typed command handlers
          -> Slack Web API client
              -> HTTP transport
      -> output renderer
```

Keep `main.rs` limited to parse, run, render error, and choose an exit code. `App` receives its client, profile store, credential store, clock, and output mode as dependencies. This permits deterministic tests without a real keychain or Slack workspace.

Use newtypes for `ChannelId`, `UserId`, `MessageTs`, `TeamId`, and `ProfileName`. A `TargetRef` enum represents an ID, `#channel`, or `@user`. A `TokenKind` enum represents bot or user tokens. Validate these types at their boundary.

`SlackClient` owns a blocking `reqwest::Client`, base URL, bearer token, user agent, and retry policy. It exposes typed methods used by first-class commands and a raw method for `api`. Every response must handle three distinct layers:

1. transport status and body;
2. Slack’s JSON envelope, including HTTP 200 with `ok: false`;
3. typed command payload.

Errors are structured by failure mode and carry the method, Slack error code, request ID when present, and safe context. Never include tokens or full request bodies in an error.

### 5.2 — Pagination, rate limits, and retries

Centralize cursor pagination. Each list command asks a paginator for up to a total limit or all results. Preserve `next_cursor` in JSON output when the command stops before exhaustion.

Slack applies rate limits per method, workspace, and app. A 429 includes `Retry-After` [10](https://docs.slack.dev/apis/web-api/rate-limits/). The client should:

- retry reads and 429 responses within a configurable 60-second elapsed budget;
- use bounded exponential backoff with jitter for connection failures before a request is written;
- never automatically replay a mutation after an ambiguous timeout or connection loss;
- show the wait on stderr unless `--json` is active;
- return exit code `6` when the retry budget is insufficient.

Do not hardcode Slack’s current tier capacities. `conversations.history` and `conversations.replies` have materially different limits for some commercially distributed non-Marketplace apps, while internal customer-built apps retain higher limits [11](https://docs.slack.dev/changelog/2025/05/29/rate-limit-changes-for-non-marketplace-apps/). Cursor handling and `Retry-After` are the durable contract.

### 5.3 — Resolution and caching

Resolve IDs without an API call. Resolve channel and user names from `conversations.list` and `users.list`. Cache only non-secret directory data under the platform cache directory, partitioned by team ID, with a five-minute TTL. `--no-cache` bypasses the cache. An exact ID always wins; an ambiguous name is an error with candidates rather than a guessed target.

### 5.4 — Output model

Command handlers return domain results rather than printing. Human and JSON renderers consume the same result. Human list output shows both names and durable IDs. Message output includes local display time and the raw Slack timestamp. JSON uses stable CLI-owned field names; `slack api` alone exposes raw Slack JSON.

Keep color optional and semantic. Never use color as the only signal. Do not add an interactive selector or terminal UI in v0.1.

---

## 6. Implementation sequence

### 6.1 — Milestone 0: capability spike

Before building the full parser, use a disposable test or small temporary binary against a user token in a test workspace. Execute `auth.test`, `conversations.list`, `conversations.history`, `conversations.replies`, `chat.postMessage`, `chat.update`, `chat.delete`, and `search.messages`. Confirm that the created message's author ID equals the user ID returned by `auth.test`, and that the same profile can edit and delete it. Repeat the visibility checks with a bot token only to define the optional bot mode.

Exit criterion: a user token posts as the installing member in a real workspace, the minimum user scopes are confirmed, and optional bot behavior is recorded separately. Update this plan and `examples/app-manifest.yaml` with the result. Do not retain spike code.

### 6.2 — Milestone 1: foundation and vertical slice

1. Rename the package and add runtime/test dependencies.
2. Add the CLI parser, `App`, output modes, errors, IDs, configuration, credential-store trait, and keyring implementation.
3. Add the Web API client with `auth.test`, error envelopes, redaction, timeouts, and test base URL injection.
4. Implement `auth add/list/status/use/remove`.
5. Implement channel list/resolution, message list, and message send.
6. Add mocked end-to-end tests covering `slack auth status`, `slack channel list`, and `slack message send`.

Exit criterion: a fresh user can create the private app from the example manifest, add the resulting user token, list `#general`, read recent messages, and send a test message by channel name that Slack attributes to that user. JSON output and all failure paths are tested.

### 6.3 — Milestone 2: complete v0.1 surface

1. Add channel info, threads, edit, delete, and permalink.
2. Add user list/info and user-target resolution.
3. Add search with a clear user-token capability error.
4. Add reaction commands.
5. Add the three-step external file upload flow.
6. Add the raw `api` command.
7. Add bounded cursor pagination, cache behavior, and method-aware retries.

Exit criterion: every command in section 3.2 works against mocked Slack responses and the non-destructive subset passes a real-workspace smoke test.

### 6.4 — Milestone 3: release hardening

1. Write README installation, app creation, scope, profile, examples, output, and security documentation.
2. Add shell completions and manpage generation at release/build time.
3. Add CI for format, Clippy, tests, dependency audit, and release builds on macOS and Linux.
4. Add changelog, license, release profile, and installation instructions.
5. Run a real destructive smoke test in a dedicated channel: send, edit, react, upload, and delete only artifacts created by the test.

Exit criterion: `cargo install --path .` produces `slack`; a clean machine can configure a profile from the README; CI is green; no secret appears in logs, config, snapshots, or errors.

---

## 7. Files touched

```text
┌──────────────────────────────────┬────────────────────────────────────────────┐
│ File                             │ Action                                     │
├──────────────────────────────────┼────────────────────────────────────────────┤
│ Cargo.toml                       │ Replace placeholder with selected stack      │
│ Cargo.lock                       │ Create resolved dependency lockfile         │
│ src/main.rs                      │ Create thin process entry point             │
│ src/app.rs                       │ Create dependency-injected orchestration     │
│ src/cli.rs                       │ Create clap command and argument model       │
│ src/config.rs                    │ Create profile metadata and precedence       │
│ src/credentials.rs               │ Create credential trait and keyring adapter  │
│ src/error.rs                     │ Create structured errors and exit codes      │
│ src/ids.rs                       │ Create validated Slack domain newtypes       │
│ src/output.rs                    │ Create human and JSON renderers              │
│ src/resolve.rs                   │ Create channel/user resolution and cache     │
│ src/slack/mod.rs                 │ Create module index and shared types only    │
│ src/slack/client.rs              │ Create Web API transport and raw call        │
│ src/slack/envelope.rs            │ Create Slack response/error envelopes        │
│ src/slack/pagination.rs          │ Create cursor pagination                     │
│ src/slack/retry.rs               │ Create method-aware retry policy             │
│ src/slack/types.rs               │ Create wire response/request types           │
│ src/commands/mod.rs              │ Create command module index only             │
│ src/commands/auth.rs             │ Create profile commands                      │
│ src/commands/channel.rs          │ Create channel commands                      │
│ src/commands/message.rs          │ Create message commands                      │
│ src/commands/search.rs           │ Create search commands                       │
│ src/commands/user.rs             │ Create user commands                         │
│ src/commands/reaction.rs         │ Create reaction commands                     │
│ src/commands/file.rs             │ Create external file upload flow             │
│ src/commands/api.rs              │ Create raw Web API escape hatch              │
│ examples/app-manifest.yaml       │ Create least-privilege app manifest example  │
│ tests/cli.rs                     │ Create process-level CLI tests               │
│ tests/support/mod.rs             │ Create mock Slack server and fixtures        │
│ README.md                        │ Create installation and usage guide          │
│ CHANGELOG.md                     │ Create release history                       │
│ .github/workflows/ci.yml         │ Create validation workflow                   │
└──────────────────────────────────┴────────────────────────────────────────────┘
```

Select dependencies during Milestone 1 against the responsibilities below. The recommended stack is `clap` for parsing, blocking `reqwest` with Rustls for HTTP, `serde`/`serde_json` for wire types, `thiserror` for structured failures, `directories` for platform paths, `toml` for profile metadata, `keyring` plus `secrecy` for credentials in memory and at rest, `rpassword` for hidden input, `url` for endpoint construction, and `time` for RFC 3339 conversion. Add a table/color crate only if plain formatting proves insufficient. Test dependencies should cover process assertions, temporary directories, and a synchronous HTTP mock server.

Pin direct versions in `Cargo.lock`, disable unused default features, and audit the resulting tree before accepting it. Do not add Tokio, an async executor, a general configuration framework, a logging framework, or a Slack SDK without a demonstrated requirement. The CLI needs a small explicit Web API client and a raw escape hatch, not a second abstraction over Slack.

---

## 8. Verification

### 8.1 — Automated checks

```text
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test --doc
cargo build --release
cargo audit
```

Tests must cover:

- every help path and required-argument conflict;
- profile precedence and missing credentials;
- token redaction from all error forms;
- HTTP 200 with `ok: false`, 401/403, malformed JSON, 429, 5xx, and timeouts;
- no mutation replay after an ambiguous transport failure;
- cursor pagination, total limits, and stopped cursors;
- ID parsing, exact and ambiguous name resolution, cache expiry, and team isolation;
- human output to stdout, diagnostics to stderr, and valid JSON under `--json`;
- message input from argument, file, and stdin;
- destructive confirmation behavior in terminal and non-terminal execution;
- file upload failure at each of its three stages.

### 8.2 — Real-workspace smoke checklist

Use a dedicated workspace or channel and both a bot and user profile:

1. Validate the user profile and confirm `auth status` identifies the intended Slack member without printing the token.
2. List public, private, DM, and MPIM conversations permitted by each profile.
3. Read history and a thread with bounded pagination.
4. Resolve a channel name and an unambiguous display name; provoke an ambiguous-name error.
5. Send a message and assert that Slack attributes it to the `auth.test` user ID; then edit, link, react to, upload into, and delete that test-owned message.
6. Search with the user profile and confirm a precise capability error with the bot profile.
7. Trigger or mock a 429 and verify `Retry-After` handling.
8. Run each read command with `--json` and parse the output with `jq`.

Never run destructive smoke tests against pre-existing messages or files.

---

## 9. Risks and decisions

### 9.1 — Decisions made

- Rust with blocking HTTP is the implementation language and runtime model.
- `slack` is the executable. On machines requiring both tools, Slack's official app-development CLI is installed or aliased as `slack-dev`.
- v0.1 is an internal operator tool using an imported user token issued by a private Slack app, not a distributed OAuth application.
- The default profile is a user token and `message send` must post as that authenticated member. Bot profiles are an explicit secondary mode for bot-owned automation.
- First-class commands cover frequent workflows; `api` covers the long tail.
- Credentials live in the OS credential store. Plaintext token storage is not supported.
- IDs are durable; name resolution is a convenience with ambiguity checks.

### 9.2 — Decisions that must be confirmed by the capability spike

- The exact least-privilege scope bundles for the workflows in section 2.1.
- Whether Slack’s current user-token installation flow is acceptable for the intended workspace policy.
- Whether private-channel, DM, and search access are required for v0.1 or can be optional capabilities.

### 9.3 — Principal risks

The largest risk is identity attribution drifting from the operator's expectation. The CLI defaults to a user token, displays the posting identity before dry runs, validates the returned author after sends, and tests the full send/edit/delete path before substantial implementation.

The second risk is scope creep from Slack’s large API. The raw `api` command and an observed-demand rule control it.

The third risk is accidental credential disclosure. Keychain storage, stdin/prompt entry, structured redaction, and adversarial error tests address it.

The fourth risk is replaying a write after an uncertain network failure. Method-aware retries prohibit automatic mutation replay unless a future endpoint supplies a proven idempotency mechanism.

---

## 10. Definition of done

v0.1 is done when:

- every command in section 3.2 is implemented and documented;
- the default user profile sends messages attributed to the same member returned by `auth.test`;
- bot profiles have separate explicit behavior and cannot be mistaken for the user profile;
- a token is never persisted outside the OS credential store or exposed in output;
- all list commands paginate correctly and respect bounded defaults;
- 429 responses obey `Retry-After`, and ambiguous writes are never replayed;
- human output is useful and `--json` is stable and parseable;
- mocked tests cover all failure classes and the real-workspace smoke checklist passes;
- `cargo fmt`, Clippy with warnings denied, tests, docs, audit, and release build pass;
- the README lets a new operator create the private Slack app, grant minimum user scopes, add the user profile, and complete a send/read workflow as themselves without undocumented steps.

---

## Amendments (2026.08.02, adopted at implementation)

1. **Two operator token paths, per profile.** A profile token is `xoxp-` (user token from a private internal app — the supported, durable path; use where we control the workspace), `xoxc-` + the `d` cookie (browser session — unsanctioned and session-lifetime, but the only route in workspaces where app installs need admin approval, e.g. client workspaces), or `xoxb-` (explicit bot automation). Both operator kinds post as the member and are subject to §4.1's identity contract: the returned message author must equal the `auth.test` user ID, and `message send --dry-run` reports the posting identity. The §6.1 spike runs against whichever kind a workspace actually uses.
2. **No keyring.** Tokens live in `~/.config/slack/config.json` mode 0600, matching gspace/xio/coresignal. macOS keychain re-prompts on every rebuilt binary signature, which is hostile to a cargo-installed personal tool. Drops `keyring`, `secrecy`; `rpassword` retained for hidden prompts.
3. **v0.1 scope cut.** Deferred to observed demand: file upload (three-step external flow), mock-server test suite, CI matrix, completions, manpages. Kept: full §2.3 UX contract, exit codes, pagination, retry policy, raw `api`.
