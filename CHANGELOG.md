# Changelog

## 0.5.0 — 2026-08-02

- Remove `auth add --curl`. Pasting a full browser request can exceed terminal
  input limits or leave credential-bearing lines queued for the shell.
- Browser-session setup now uses the normal hidden `auth add` prompts: copy the
  `xoxc` token from Network → Payload and the `d` cookie from Application →
  Cookies. The README separates user, browser-session, and bot authentication.

## 0.4.1 — 2026-08-02

- Fix: `reaction add/remove` resolved targets through the channel path, so a
  DM target (`@user` or a user ID) failed with `channel_not_found`. It now
  uses the same target grammar as `message`. Found by the first real
  end-to-end write round trip.

## 0.4.0 — 2026-08-02

- `slack message send --attach <path>` (repeatable) sends a message with
  files in one operation — the text rides as the upload's `initial_comment`,
  so this is one Slack call, not a send plus an upload. `--dry-run` lists
  each attachment with its size, or flags it unreadable. `file upload`
  remains as the file-first alias over the same path.

## 0.3.0 — 2026-08-02

- `slack file upload <channel> <path>` via the three-step external upload
  flow (`files.upload` was retired 2025-11-12), with `--title`, `--comment`,
  and `--thread`. The pre-signed PUT is never retried, so a failure cannot
  duplicate the upload.
- Fix: DM results in `search messages` rendered as `#U0B…` because Slack puts
  the partner's user ID in `channel.name` for DMs. Now shows `@Display Name`.

## 0.2.0 — 2026-08-02

- `slack auth add <name> --curl` extracts the `xoxc` token and `xoxd` cookie
  from a pasted DevTools "Copy as cURL" command, replacing the two-place
  manual hunt. Format-agnostic across Chrome/Firefox/Safari output; cookie
  preserved URL-encoded. A browser-console one-liner is impossible — the `d`
  cookie is HttpOnly and unreadable from page scripts.

## 0.1.0 — 2026-08-02

Initial release.

- Profiles for multiple workspaces (`auth add/list/status/use/remove`),
  supporting `xoxp` user tokens, `xoxc`+`d`-cookie browser sessions, and
  `xoxb` bot tokens. Stored 0600 in `~/.config/slack/config.json`.
- `channel list/info`, `message list/thread/send/edit/delete/permalink`,
  `search messages`, `user list/info`, `reaction add/remove`, and a raw
  `api` escape hatch (form params or `--data` JSON).
- Name resolution (`#channel`, `@user`) with 5-minute directory cache,
  ambiguity errors with candidates, IDs always pass through.
- Identity contract: sends by operator tokens verify the posted author
  matches the profile's `auth.test` user id; `--dry-run` reports the
  posting identity.
- Stable exit codes (0/2/3/4/5/6), cursor pagination with `--limit`/`--all`,
  429 retries within a 60s `Retry-After` budget, no mutation replay.
