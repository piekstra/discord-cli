# Discord REST API notes

Everything goes through `https://discord.com/api/v10` with
`Authorization: Bot <token>` and a `DiscordBot (repo-url, version)` user
agent. Official docs: <https://discord.com/developers/docs>.

## Endpoints used

| Command | Endpoint |
|---------|----------|
| `me`, `auth login` (verify) | `GET /users/@me` |
| `guilds` | `GET /users/@me/guilds` (first page, ≤ 200 guilds) |
| `channels` | `GET /guilds/{guild.id}/channels` |
| `messages` | `GET /channels/{channel.id}/messages?limit=&before=&after=` |
| `send` | `POST /channels/{channel.id}/messages` (`content`, optional `message_reference.message_id`) |
| `delete` | `DELETE /channels/{channel.id}/messages/{message.id}` (204 → null) |
| `api` | anything — raw passthrough |
| `app get`, `app intents show` | `GET /applications/@me` |
| `app intents enable/disable` | `PATCH /applications/@me` (`flags`) |
| `app invite-url` | none — built locally from the app ID |

## Application flags / privileged intents

`flags` on the application object carries the privileged-intent state. Each
intent has a *full* bit (granted at verification, portal-only) and a
`*_LIMITED` bit (apps under 100 servers, **API-editable**):

| Intent | Full | Limited |
|--------|------|---------|
| presence | `1<<12` | `1<<13` |
| guild members | `1<<14` | `1<<15` |
| message content | `1<<18` | `1<<19` |

`PATCH /applications/@me` accepts only the limited bits; send the complete
flags value with the bit toggled. This is how `discord app intents enable
message-content` replaces the portal checkbox.

## Portal operations (`app list` / `app create` / `app reset-token`)

Creating applications, listing them, and resetting bot tokens have **no
bot-token API** — the developer portal drives undocumented user-session
endpoints on `/api/v9`. `portal/portal.mjs` (staged and npm-installed into
`~/.cache/discord-cli/portal/` by `src/portal.rs`) opens a persistent,
headful Chromium where the user logs in; it then:

- captures the portal's `Authorization` header from the page's own API
  traffic (in-process only; never persisted or printed);
- **auto-login is lazy:** it reads the user's login from `op` (item name via
  `DISCORD_CLI_PORTAL_OP_ITEM`) **only if the login form actually appears** —
  an already-logged-in session never triggers a keychain/`op` prompt. It also
  dismisses the "Welcome to the Developer Portal" modal (its "Log In" button)
  before filling the form;
- `list` → `GET /api/v9/applications?with_team_applications=true`;
- `create` → drives the New Application dialog (ToS checkbox ticked via its
  react-aria indicator, since Discord's direct `POST /applications` 400s
  without the ToS/hCaptcha) and captures the resulting POST response; the
  human clears the **hCaptcha** in the window;
- `reset-token` → clicks Reset Token on the app's Bot page, auto-fills the
  **MFA password** from `op` when available (else the human does it), and
  captures the `POST …/bot/reset` **response**. The fresh token goes to
  stdout (a pipe to the parent CLI), which verifies it against `/users/@me`
  and stores it in the keychain.

These endpoints are undocumented and may drift; the human-in-the-window
fallback (do the clicks yourself, the script only harvests the network
response) keeps `reset-token` and `create` usable through UI changes.

## Gotchas

- **Message Content Intent.** Without the privileged *Message Content
  Intent* (developer portal → Bot), `content`, `embeds`, and `attachments`
  come back **empty** for most messages — the request still succeeds. The
  exceptions are DMs, messages that mention the bot, and the bot's own
  messages. Below 100 servers this is a checkbox; beyond that it needs
  Discord's approval.
- **Permissions vs authentication.** A valid token can still get HTTP 403
  (`Missing Access`, code 50001) if the bot wasn't invited to the guild or
  can't see the channel. We map 401 *and* 403 to exit 3 (auth); the error
  text carries Discord's message + code.
- **Rate limits.** 429 responses include `Retry-After` (seconds, may be
  fractional). `client.rs` waits out up to two short (≤ 5s) limits per
  request, then surfaces exit 5. Global limit is 50 req/s per bot — nothing
  this CLI approaches.
- **Snowflakes.** All IDs are u64-as-string; keep them strings end-to-end
  (JSON numbers would lose precision in some consumers). `limit` for message
  fetches is 1–100 (default 50 server-side; we default 25).
- **Content cap.** Message `content` is ≤ 2000 characters for bots without
  premium; we enforce this client-side for a friendly usage error.
- **204s.** Some mutations (e.g. `DELETE /channels/{id}/messages/{id}` via
  `discord api`) return `204 No Content`; the client renders that as JSON
  `null`.
