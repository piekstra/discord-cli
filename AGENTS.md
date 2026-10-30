# AGENTS.md — discord-cli

Canonical agent entrypoint for this repo. `CLAUDE.md` is a one-line pointer here.

## What this is

A single-binary CLI (`discord`) over the **official Discord REST API (v10)**
using bot-token auth — send and read channel messages, list guilds and
channels, plus a raw `api` passthrough. Design ergonomics: keychain-only
runtime secrets with stdin/env ingress, text-primary output, `--json`
everywhere, and stable exit codes.

The endpoints used and Discord-specific gotchas (privileged intents, rate
limits, permissions) are in [`docs/api.md`](docs/api.md) — read it before
touching `src/client.rs`.

## Local map

| Path | Responsibility |
|------|----------------|
| `src/main.rs` | thin entrypoint; parses args, dispatches |
| `src/cli.rs` | `clap` command tree; `BIN` / `REPO` constants |
| `src/client.rs` | `Discord` HTTP client: auth header, rate-limit retry, error mapping, endpoints |
| `src/commands/*.rs` | one handler per command group (auth, me, guilds, channels, messages, send, delete, app, config, api, info) |
| `src/commands/mod.rs` | shared `Ctx` + guild/channel ID-or-name resolution |
| `src/config.rs` | `~/.config/discord/config.json` shape (non-secret prefs) |
| `src/portal.rs` | stages + runs the portal helper (npm bootstrap, JSON protocol, exit-code mapping) |
| `portal/portal.mjs` | Playwright script: headful browser, human login, app list/create/token-reset via network capture |
| `build.rs` | bakes the target triple into `BUILD_TARGET` for self-update |

## Durable conventions (do not drift)

- **Secrets: keychain-only at runtime.** The bot token is read only from the
  OS keychain (service `piekstra.discord`, account `bot-token`). It gets there
  via `auth login` / `auth set-credential`, which ingest from `--stdin`,
  `--from-env <VAR>`, or a no-echo prompt — **never a `--token` flag** (leaks
  into `ps`, history, transcripts). Wrap secrets in `pk_cli_secrets::Secret`;
  never log or serialize one.
- **Bot accounts only.** This CLI authenticates with `Authorization: Bot …`.
  Do not add user-token ("self-bot") support — it violates Discord's ToS.
- **1Password auto-login is lazy and pass-through only.** The parent passes
  the `op` item **name** to the helper via `DISCORD_CLI_PORTAL_OP_ITEM` (a
  name, not a secret); `portal.mjs` reads the login from `op` at the moment
  of use — and only if it actually hits the login/MFA form, so an
  already-logged-in session never prompts. The secret values (op stdout) stay
  in memory, never stored, logged, printed, or put on argv/env-values. Only
  the item *name* may persist (config `op-item`). Do not reintroduce an
  eager, up-front credential fetch.
- **Portal ops stay interactive and headful.** `app list/create/reset-token`
  open a visible browser where the human logs in and completes any MFA in
  Discord's own UI. Never make them headless, never persist/log/print the
  captured user-session header, and never extend them beyond managing the
  user's own applications (no chat actions as the user, ever). Prefer
  network-response capture over DOM scraping — the human doing the clicks
  must remain a working fallback.
- **IDs or names.** Guild/channel arguments accept a snowflake or a name;
  name resolution must fail loudly on ambiguity (never pick one silently).
- **Output contract.** Text is primary (chat-log lines for messages, tables
  for lists); `--json` emits exactly one schema-tagged DTO on stdout. Data →
  stdout, diagnostics/confirmations → stderr.
- **Exit codes are a contract:** `0` ok, `1` other/keychain, `2` usage, `3`
  auth/permissions, `4` not found, `5` upstream, `6` confirmation required.
- **Best-effort parsing.** Never `unwrap()` on a Discord response field;
  shapes vary by channel type and API evolution.

## This repo is public

Never commit personal data — no real bot token, guild/channel/user IDs from
private servers, or message content, in code, tests, fixtures, comments, or
commit messages. Use placeholder snowflakes like `1112223334445556667`. CI
runs `gitleaks`.

## Local checks (must pass before pushing)

```sh
make verify        # fmt-check + clippy -D warnings + test + smoke
make audit         # cargo-deny (licenses/advisories), matches CI
```

`make smoke` needs no login and exercises the binary end-to-end.
Authenticated paths can't run in CI; keep their logic covered by unit tests
on the pure helpers and verify manually with `discord api`.

## The CLI family & cli-common

This CLI conforms to **piekstra-cli/1** — the shared surface spec in
[piekstra/cli-common](https://github.com/piekstra/cli-common) (`DESIGN.md`):
standard `auth` / `config` / `self-update` / `completions` / `info` commands,
global `--json`, canonical DTOs (`auth-status/v1`, `self-update/v1`,
`cli-info/v1`), and frozen exit codes 0–6.

- **Don't fork shared behavior.** Error/exit-code handling, output rendering,
  keychain secrets, config storage, and self-update come from the `pk-cli-*`
  crates (tag-pinned git deps on cli-common). If you need a change there — or
  you're writing anything reusable across the family CLIs — add it to
  cli-common, cut a tag, and bump the pin here. Never copy shared code into
  this repo.
- **Surface changes are spec changes.** A new standard command, flag, DTO
  field, or exit code belongs in cli-common's `DESIGN.md` first; update
  `conformance.md` alongside.
- **macOS dev signing.** Every plain `cargo build` gets a fresh ad-hoc code
  signature, so keychain "Always Allow" grants don't stick and every rebuild
  re-prompts. One-time: run cli-common's `scripts/setup-dev-signing.sh`. Then
  build with `make dev` whenever you'll exercise keychain-touching commands.
