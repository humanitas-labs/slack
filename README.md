# slack

Operate Slack from the command line. Read conversations, send messages as
yourself, search, react, and share files across multiple workspaces.

```sh
cargo install --path .
```

## Authenticate

You authenticate once per workspace; each is saved under a name you choose.

The supported path is a user token from a private Slack app:

1. Create an app from [`examples/app-manifest.yaml`](examples/app-manifest.yaml).
2. Install it to the workspace and copy its **User OAuth Token** (`xoxp-…`).
3. Add and verify the profile:

```sh
slack auth add <workspace>
slack auth status
```

For a workspace where you cannot install an app, the CLI can use your browser
session. Copy any signed-in Slack API request from DevTools with **Copy as
cURL**, then run:

```sh
slack auth add <workspace> --curl
```

On macOS, copy the request before running the command. The CLI reads the
clipboard immediately; do not paste into the terminal or press Return afterward.
You can also pipe a copied request into the command on any platform. `--curl`
scans it for the `xoxc` token and `xoxd` cookie, so you copy one thing instead of
hunting two.

This `xoxc`/cookie path is unsupported by Slack and expires with the browser
session. Prefer an `xoxp` user token. Bot tokens (`xoxb-…`) are accepted for
explicit bot automation.

Workspaces are stored in `~/.config/slack/config.json` with mode `0600`.

```sh
slack auth list
slack auth use <workspace>
slack auth remove <workspace>
```

## Use

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

## Security

- Tokens and browser cookies are stored locally and never printed.
- `--dry-run` shows the resolved target and posting identity.
- Destructive commands prompt interactively and require `--yes` in scripts.
- Browser-session authentication uses Slack's internal interface. Use it only
  when the supported app-token path is unavailable.

Slack also distributes an app-development CLI named `slack`. If both tools are
installed, alias the official developer CLI as `slack-dev`.
