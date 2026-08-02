# slack

Operate Slack from the command line. Read conversations, send messages as
yourself, search, react, and share files across multiple workspaces.

```sh
cargo install --path .
```

## Commands

```sh
# Read
slack channel list
slack message list '#general' --limit 50
slack message thread '#ops' 1754160000.000100
slack search messages 'from:andrew in:#shipping regression'

# Write
slack message send '#ops' 'deploy done'
slack message send '@ada' --text-file message.md
slack message send '#ops' 'reply' --thread 1754160000.000100
slack message send '#ops' 'this week' --attach chart.png
slack reaction add '#ops' 1754160000.000100 white_check_mark

# Inspect before sending
slack message send '#ops' 'deploy done' --dry-run

# Call an unwrapped Web API method
slack api conversations.members channel=C01234567 limit=200
```

Channel and user IDs always work. `#channel` and `@user` references resolve by
name and fail on ambiguity. Use `--workspace <name>` to select a workspace and
`--json` for machine-readable output.

Run `slack --help` or `slack <command> --help` for the complete command and
flag reference.

## Authenticate

You authenticate once per workspace; each is saved under a name you choose.

### A) User token (recommended)

Use a user token when you can install a private Slack app. It is supported by
Slack and sends messages as you.

1. Create an app from
   [`docs/examples/app-manifest.yaml`](docs/examples/app-manifest.yaml).
2. Install it to the workspace and copy its **User OAuth Token** (`xoxp-…`).
3. Add and verify the workspace:

```sh
slack auth add <workspace>
slack auth status
```

### B) Browser session (no app)

Use an existing browser session when you cannot install an app in the
workspace. Copy two values directly from DevTools:

1. Open the Network tab, select a Slack `/api/` request, open its Payload tab,
   and copy the `token` value (`xoxc-…`).
2. Open Application → Cookies, select your Slack workspace, and copy the value
   of the `d` cookie (`xoxd-…`).
3. Run the command below and paste each value into its hidden prompt.

```sh
slack auth add <workspace>
```

This `xoxc`/cookie path is unsupported by Slack and expires with the browser
session. Prefer a user token when the workspace allows it.

### C) Bot token

Use a bot token only for automation that should act as the app rather than as
you. Copy the app's **Bot User OAuth Token** (`xoxb-…`), then run:

```sh
slack auth add <workspace>
slack auth status
```

### Manage workspaces

Workspaces are stored in `~/.config/slack/config.json` with mode `0600`.

```sh
slack auth list
slack auth use <workspace>
slack auth remove <workspace>
```

## Security

- Tokens and browser cookies are stored locally and never printed.
- `--dry-run` shows the resolved target and posting identity.
- Destructive commands prompt interactively and require `--yes` in scripts.
- Browser-session authentication uses Slack's internal interface. Use it only
  when the supported app-token path is unavailable.

Slack also distributes an app-development CLI named `slack`. If both tools are
installed, alias the official developer CLI as `slack-dev`.
