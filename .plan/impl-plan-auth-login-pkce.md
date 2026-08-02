---
title: "slack auth login — user-token OAuth with PKCE"
date: 2026-08-02
status: draft
affects: authentication / workspace setup
---

## Context

Setup today has two paths (`src/commands/auth.rs`):

- **`auth add` (xoxp)** — the supported path, but manual: create a Slack app,
  install it, copy the User OAuth Token, paste it. Repetitive per workspace.
- **`auth add --curl`** — the unsupported browser-session fallback for
  workspaces where an app can't be installed. Kept.

We rejected `--from-app` (Slack Desktop LevelDB parsing) — tokens are
snappy-compressed, only one extracts reliably, and the code reads like
credential theft.

The remaining upgrade is Slack's PKCE OAuth (GA 2026.03.30), which lets a
public client (a CLI) run the standard browser consent flow with **no
`client_secret`**, yielding a **user token** (bot scopes are prohibited under
PKCE desktop redirects — irrelevant, we want a user token). This turns the
xoxp path from "make an app, copy a token" into one command: `slack auth
login`.

### The app registration — what's still required

PKCE removes the secret, **not** the app. We register **one** digimata app,
once, mark it distributable, and ship its **`client_id`** in the binary
(a client ID is not a secret). After that:

- **Our own workspaces:** `slack auth login` → approve consent → done. No
  per-workspace app, no token copying.
- **Client workspaces:** their admin must still approve the app being added.
  Absent that approval, OAuth hits the same wall as today → fall back to
  `--curl`. `login` does not change the client story.

## Changes

1. **One-time manual setup (Andrew, in browser — not code).**
   - Register a Slack app (reuse `examples/app-manifest.yaml` scopes).
   - Enable **PKCE / public client** in app settings.
   - Add redirect URL `http://localhost` (Slack allows a loopback redirect
     with a dynamic port for PKCE; confirm exact form in app settings — may
     need a fixed port like `http://localhost:37421/callback`, in which case
     the listener binds that port).
   - Set app distribution to public/unlisted so non-admin members can request
     approval.
   - Record the `client_id`. This is the only manual step, and it's one-time.

2. **`src/auth/pkce.rs` (new).** PKCE primitives.
   - `verifier()` — 64+ chars of unreserved-charset randomness.
   - `challenge(verifier)` — base64url(SHA-256(verifier)), no padding.
   - `state()` — random anti-CSRF nonce.
   - Randomness: `Date.now()`/`rand` — use the `getrandom` crate (already a
     transitive dep via rustls) directly, or add `rand`. Decision: use
     `getrandom` to avoid a new top-level dep. SHA-256 via `sha2` (add).
   - base64url via `base64` (add) — small, no-std capable.

3. **`src/auth/oauth.rs` (new).** The login flow, blocking (matches the
   codebase's blocking reqwest model).
   - `login(client_id, scopes) -> Profile`:
     1. Generate verifier/challenge/state.
     2. Bind a one-shot `TcpListener` on `127.0.0.1:<port>` (std lib; no async
        runtime — accept one connection, parse the `GET /callback?...` line,
        return a tiny HTML "you can close this tab" page).
     3. Build the authorize URL:
        `https://slack.com/oauth/v2/authorize?client_id=…&user_scope=…&`
        `redirect_uri=…&code_challenge=…&code_challenge_method=S256&state=…`
     4. Open it with `open`/`xdg-open`/`start` (small platform match; no crate
        needed).
     5. Accept the redirect, verify `state` matches, extract `code`.
     6. Exchange via existing `Client` against `oauth.v2.access` with
        `client_id`, `code`, `redirect_uri`, `code_verifier`, `grant_type=
        authorization_code`. The user token is in `authed_user.access_token`
        (`xoxp-…`), not the top-level `access_token`.
     7. `auth.test` with the new token → fill identity, save `Profile`.
   - Timeout the listener (~2 min) so a closed browser doesn't hang forever.

4. **`src/cli.rs` — `AuthCmd::Login`.**
   ```
   Login {
       name: String,
       /// Override the built-in client_id (client-owned app)
       #[arg(long)] client_id: Option<String>,
       /// Fixed callback port (default: OS-assigned or the registered port)
       #[arg(long)] port: Option<u16>,
   }
   ```
   Supporting a `--client-id` override is what lets a client install *their*
   app and have users log into it — GPT's "client-owned app" model, near-free
   to add now.

5. **`src/commands/auth.rs` — wire `Login`.** New `login()` fn: resolve
   client_id (flag > built-in const), call `oauth::login`, save the workspace
   exactly like `add()` does (share the save/describe tail — refactor the
   workspace-persist block of `add()` into a helper both call).

6. **Built-in client_id constant.** `const DIGIMATA_CLIENT_ID: &str = "…";`
   in `oauth.rs` or a `src/auth/mod.rs`. Documented as non-secret.

7. **Docs.** README: lead the Authenticate section with `slack auth login`;
   demote manual xoxp paste to "or, add an existing token"; keep `--curl` as
   fallback. CHANGELOG 0.5.0 entry. `examples/app-manifest.yaml`: add a note
   that the same manifest + PKCE toggle backs `login`.

## Files touched

```
┌────────────────────────────┬───────────────────────────────┐
│            File            │            Action             │
├────────────────────────────┼───────────────────────────────┤
│ src/auth/mod.rs            │ Create (module + client_id)   │
│ src/auth/pkce.rs           │ Create (verifier/challenge)   │
│ src/auth/oauth.rs          │ Create (login flow + listener)│
│ src/cli.rs                 │ Edit (AuthCmd::Login)         │
│ src/commands/auth.rs       │ Edit (wire login, share save) │
│ src/main.rs                │ Edit (mod auth; dispatch)     │
│ Cargo.toml                 │ Edit (sha2, base64[, rand])   │
│ README.md                  │ Edit (login-first setup)      │
│ CHANGELOG.md               │ Edit (0.5.0)                  │
│ examples/app-manifest.yaml │ Edit (PKCE note)              │
└────────────────────────────┴───────────────────────────────┘
```

## Open questions / risks

- **Redirect URI form.** Slack's PKCE loopback rules decide whether we get a
  dynamic port or must register a fixed one. Confirm in app settings before
  coding the listener bind. This gates step 3.2/3.3.
- **Token location in the response.** User-token PKCE returns the token under
  `authed_user.access_token`; verify against a live exchange, don't assume the
  top-level field.
- **Scope approval.** Broad user scopes may trigger admin-approval prompts on
  managed workspaces — expected, not a bug; surface Slack's message verbatim.

## Verification

- `cargo build`, `cargo clippy`, `cargo test` clean (add unit tests for
  `pkce::challenge` against a known RFC 7636 test vector).
- Live: register the app, `slack auth login test` against digimata's own
  workspace, approve consent, confirm a workspace is saved and `slack auth
  status test` reports the right identity.
- `slack message send '@self' 'login flow works'` — leave it up for review,
  don't auto-delete.
- Confirm `--curl` and `auth add` paths still work unchanged.
