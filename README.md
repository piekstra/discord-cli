# discord-cli

Interact with Discord from the command line — send and read messages, list
servers and channels. Binary: `discord`. Bot-token auth secured by the OS
keychain, `--json` on every command, stable exit codes, agent-friendly.
Unofficial; not affiliated with or endorsed by Discord.

Part of the [piekstra CLI family](https://github.com/piekstra/cli-common)
(**piekstra-cli/1**): same `auth` / `config` / `self-update` / `completions` /
`info` surface as `fpl`, `tojfl`, `lrfl`, and `xfin`.

```console
$ discord send "#general" "deploy finished ✅"
sent message 1234567890123456789 to channel 1112223334445556667

$ discord messages "#general" -n 3
[2026-07-15 17:18] caleb (1394200…): kicking off the deploy
[2026-07-15 17:20] deploy-bot (1234567…): deploy finished ✅
[2026-07-15 17:21] caleb (1394202…): 🎉
```

## Install

From a GitHub release (macOS arm64/x86_64, Linux x86_64):

```sh
curl -sL https://github.com/piekstra/discord-cli/releases/latest/download/discord-aarch64-apple-darwin.tar.gz | tar xz
mv discord ~/bin/  # or anywhere on $PATH
```

Or from source: `cargo install --git https://github.com/piekstra/discord-cli`.
Later, `discord self-update` keeps the binary current.

## Setup

The credential is a **Discord bot token** — this CLI drives a bot user, not
your personal account (automating user accounts violates Discord's ToS).

1. Create an application at
   <https://discord.com/developers/applications> → **New Application**.
2. Under **Bot**: *Reset Token* and copy the token. To read message content
   with `discord messages`, also enable the **Message Content Intent** on the
   same page (otherwise content comes back empty).
3. Invite the bot to your server (as a server admin) with *View Channels*,
   *Send Messages*, and *Read Message History*:

   ```
   https://discord.com/oauth2/authorize?client_id=<YOUR_APP_ID>&scope=bot&permissions=68608
   ```

4. Store the token (keychain-only; never passed as a flag):

   ```sh
   discord auth login                       # interactive no-echo prompt
   op read op://Private/discord-bot/token | discord auth login --stdin
   discord config set guild "My Server"     # default guild for #name lookups
   ```

## Usage

```sh
discord me                            # the bot user (verifies the token)
discord guilds                        # servers the bot has joined
discord channels                      # channels of the default guild
discord channels "My Server"          # …or by guild name / ID
discord messages "#general" -n 50     # recent messages, oldest first
discord messages 1112223334445556667 --before 1234567890123456789
discord send "#general" "hello"       # by #name (needs a guild) or channel ID
some-command | discord send "#alerts" # message text from stdin
discord send "#general" --reply-to 1234567890123456789 "replying"
discord delete "#general" 1234567890123456789  # confirms; --force to skip
discord api GET /users/@me            # raw REST v10 passthrough (always JSON)
discord api POST /channels/<ID>/messages --data '{"content":"hi"}'
```

Channels and guilds are accepted by ID (snowflake) or by name; `#name`
resolution uses `--guild` or the configured default guild and fails loudly on
ambiguity (though a text channel is preferred over a same-named voice channel
for `messages`/`send`/`delete`).

## Application management

Once a bot token is stored, the app behind it can be managed via the
official API — no portal visit needed:

```sh
discord app get                            # id, name, flags, intent state
discord app intents show
discord app intents enable message-content # unverified apps (< 100 servers)
discord app invite-url                     # OAuth2 invite link (default perms 68608)
discord app invite-url --permissions 8     # e.g. administrator
```

Creating apps, listing them, and minting bot tokens have no bot API, so
those commands open a real, visible browser (Playwright, requires Node.js):
**you** log into Discord in that window — password/MFA go only into
Discord's own UI — and the helper automates just your app management:

```sh
discord app list                # your applications
discord app create my-bot       # new application
discord app reset-token [ID]    # mint a fresh bot token → keychain, verified
discord app logout              # forget the cached browser login
```

With the 1Password CLI (`op`), the browser can log itself in — username,
password, and TOTP are read from `op` **only if the login form actually
appears** (an already-logged-in session never prompts), and never touch
argv, env values, or disk:

```sh
discord config set op-item discord.com   # your 1Password Login item
discord app list                          # or --op-item <ITEM> per call
```

Captchas or new-location checks simply fall back to you finishing the
login in the window. Creating an app is always captcha-gated by Discord, so
`app create` drives the dialog and waits for you to clear the "Are you human?"
check in the window.

First run installs Playwright into `~/.cache/discord-cli/portal` (one-time
npm install; the Chromium download is skipped if already cached). The
browser profile persists there so later runs skip the login. A fresh
server's zero-portal path:

```sh
discord app create my-bot          # log in once in the window
discord app reset-token            # token lands in the keychain
discord app intents enable message-content
discord app invite-url             # open this link to invite the bot
```

## Agent / script contract

Every command takes `--json` and emits exactly one DTO on stdout
(`user/v1`, `guilds/v1`, `channels/v1`, `messages/v1`, `message/v1`,
`deleted/v1`, `app/v1`, `apps/v1`, `intents/v1`, `invite-url/v1`,
`auth-status/v1`, `self-update/v1`, `cli-info/v1`); errors become
`{"error": {"code", "message"}}`. Diagnostics go to stderr. Exit codes are
frozen: `0` ok · `1` other · `2` usage · `3` auth/permissions · `4` not
found · `5` upstream/network · `6` confirmation required.

```console
$ discord --json auth status
{
  "schema": "auth-status/v1",
  "required": true,
  "authenticated": true,
  "method": "password",
  "username": "deploy-bot",
  "credential_in_keychain": true
}
```

`discord info` describes the CLI (`cli-info/v1`) for driver tools.

## Security

- The bot token lives **only in the OS keychain** (service
  `piekstra.discord`), written via stdin, `--from-env <VAR>`, or an
  interactive no-echo prompt — never a `--token` flag, never a file.
- Non-secret preferences live at `~/.config/discord/config.json`.
- Requests go directly to `discord.com` over TLS (rustls); there is no
  telemetry and no third-party service.
- Portal commands capture your user-session header in-process only — it is
  never written to disk, logged, or printed. What does persist is the
  browser profile at `~/.cache/discord-cli/portal/profile` (a normal
  logged-in browser, like Chrome's own profile); `discord app logout`
  deletes it.

## Development

```sh
make verify   # fmt-check + clippy -D warnings + test + smoke
make audit    # cargo-deny licenses/advisories (matches CI)
make dev      # debug build re-signed so macOS keychain grants stick
```

See [AGENTS.md](AGENTS.md) for repo conventions and
[docs/api.md](docs/api.md) for the endpoint map.

## License

MIT OR Apache-2.0, at your option.
