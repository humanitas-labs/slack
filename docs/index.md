# Slack CLI documentation

`slack` is a Rust command-line client for operating Slack as a user or bot. It
supports multiple workspaces, readable terminal output, stable JSON output, and
the raw Web API when a dedicated command is not yet available.

The [README](../README.md) is the user-facing quick start. This index describes
the project documentation and the implementation behind the command surface.

## Documentation map

| Path | Purpose |
| --- | --- |
| [`README.md`](../README.md) | Installation, command overview, authentication, and safety behavior. |
| [`docs/index.md`](index.md) | Documentation entry point and architecture overview. |
| [`docs/examples/`](examples/) | Configuration and setup examples, including the private [Slack app manifest](examples/app-manifest.yaml). |
| [`docs/.ideas/`](.ideas/) | Design notes for possible features that are not yet implementation commitments. |
| [`.plan/`](../.plan/) | Approved or proposed implementation plans. |
| [`.issues/`](../.issues/) | Local Markdown projection of the GitHub issue backlog. |

## Application architecture

The application is a single blocking Rust binary. Commands share a workspace
context, directory resolver, and Slack Web API client rather than implementing
their own authentication or transport behavior.

```text
argv
  → CLI parser
  → command dispatcher
  → workspace configuration
  → channel/user resolver
  → Slack Web API client
  → terminal or JSON output
```

| Component | Responsibility |
| --- | --- |
| [`src/main.rs`](../src/main.rs) | Parse arguments, execute the selected command, render errors, and return stable exit codes. |
| [`src/cli.rs`](../src/cli.rs) | Define the public command and option grammar with Clap. |
| [`src/commands/`](../src/commands/) | Implement authentication, channels, messages, search, users, reactions, files, and the raw API escape hatch. |
| [`src/config.rs`](../src/config.rs) | Store named workspace credentials and resolve the active workspace. |
| [`src/resolve.rs`](../src/resolve.rs) | Resolve `#channel`, `@user`, and Slack IDs using a per-workspace directory cache. |
| [`src/slack/`](../src/slack/) | Handle Web API requests, Slack response envelopes, pagination, rate limits, uploads, and response types. |
| [`src/output.rs`](../src/output.rs) | Convert Slack timestamps and markup into readable terminal output or stable JSON. |
| [`src/error.rs`](../src/error.rs) | Define typed failures and the CLI exit-code contract. |

## Runtime model

Authentication is workspace-scoped. The preferred credential is an `xoxp`
user token; `xoxb` bot tokens are supported for bot-owned automation, and an
`xoxc` token plus `d` cookie can reuse a browser session when an app cannot be
installed.

| Local path | Contents |
| --- | --- |
| `~/.config/slack/config.json` | Named workspace credentials and identity metadata. Written with mode `0600` on Unix. |
| `~/.cache/slack/<workspace>/` | Channel and user directory caches with a five-minute lifetime. |

Workspace selection follows this precedence: `--workspace`, `SLACK_TOKEN`,
`SLACK_WORKSPACE`, configured default, then the sole configured workspace.
Literal Slack IDs bypass name lookup. Name resolution fails on ambiguity rather
than guessing.

## Design rules

- User and session tokens post as the authenticated member; bot tokens act as
  the installed app.
- Destructive commands require confirmation and `--yes` for non-interactive
  use.
- `--dry-run` exposes resolved targets and posting identity before mutation.
- Slack `429` responses honor `Retry-After` within a bounded retry budget.
- Transport failures and pre-signed file uploads are not replayed.
- `--json` is the machine-readable interface; ordinary output is optimized for
  terminal use.

The current proposed extension is [Slack render](.ideas/slack-render.md), an
incremental Markdown projection of selected conversations. Its tracked delivery
work lives in [issue #2](../.issues/iss-0002.md).
