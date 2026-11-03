# Security policy

## Reporting a vulnerability

Report security issues privately, not in a public issue: use "Report a vulnerability" under
this repository's Security tab. We aim to acknowledge within a few days.

## What this tool holds

| Where | Holds |
|---|---|
| OS keychain (`piekstra.discord`, `bot-token`) | Your bot token, from `discord auth login` |
| OS keychain (`piekstra.discord`, `mcp-http-token`) | Only after `discord mcp --new-token`: the bearer token of `mcp --http` |
| `${XDG_CONFIG_HOME:-~/.config}/discord/config.json` (on Windows, `~` is `%USERPROFILE%`) | A default server, the bot's username, and a 1Password item's name. No secret. |
| `~/.cache/discord-cli/portal/profile` | The browser profile of the `discord app` portal commands, a normal logged-in browser. `discord app logout` deletes it. |

A token enters by stdin, `--from-env <VAR>` or a no-echo prompt, never as an argument. Requests
go to `discord.com` over TLS. A token is never sent over plain `http`, and a redirect that
leaves `https` is refused.

## MCP tools

`discord mcp` serves three read-only tools to Claude: `discord_servers`, `discord_channels` and
`discord_messages`. **No tool sends, edits, deletes or moderates anything, and none takes a path
or a URL.** That is deliberate: a model reads text here that anyone in a channel wrote, so a
send tool would be steerable by that text. Every request a tool makes is a `GET`, and the tests
hold the requests to that. A channel or server name is only looked up, and only an all-digit id
reaches a URL.

The token is read once per call, from `DISCORD_BOT_TOKEN` (how the Claude Desktop extension
passes it, from its sensitive settings) and then the keychain, and is not held between calls.
Listing the tools touches neither.

**What reaches Claude.** Tool results go to Claude as part of your conversation, and they hold
other people's words and usernames: everything your bot can see in the channels you ask about.
Read only servers where that is acceptable. Message text is third-party data; the server's
instructions tell the model to treat it as data, never as instructions, and to tell you what a
message asks for rather than doing it. Attachment links are signed, so no result carries them.

`discord mcp --http --port <PORT>` serves the same tools for your own Cloudflare Worker, which
reaches it through your own Cloudflare Tunnel (cli-common's `connector/`). It binds 127.0.0.1
only, with no flag for another address, and refuses to start without a bearer token of 32 or
more characters, from `DISCORD_MCP_TOKEN` or the keychain item `mcp-http-token`, which `discord
mcp --new-token` makes from 256 random bits and replaces only with `--overwrite`. Every request
must carry the token (compared in constant time) and a `Host` of `127.0.0.1` or `localhost`, and
must not carry an `Origin`; there are no CORS headers. A copy of that token lives in your Worker
as its `HOME_DISCORD_TOKEN` secret. A leaked copy can read whatever your bot can see, and cannot
send anything or obtain the bot token.

**Tool results also cross Cloudflare on this path**, through your tunnel and your Worker, on
their way to Claude. Who holds what, and what a leaked token can do, is in the connector's
[threat model](https://github.com/piekstra/cli-common/blob/main/connector/README.md#threat-model).

To revoke the connector's access, run `discord mcp --new-token --overwrite` and restart the
server (`launchctl kickstart -k gui/$(id -u)/com.piekstra.discord-mcp-http`); the old token
works until the restart, and a token in `$DISCORD_MCP_TOKEN` wins until you change it. To stop
serving, run `launchctl bootout gui/$(id -u)/com.piekstra.discord-mcp-http` and stop the tunnel.
`discord auth logout --forget` removes the bot token and the connector token from the keychain;
a running server keeps its connector token until it stops. Reset the bot's token on Discord's
developer portal if you think it leaked.

## Supported versions

Before 1.0, only the latest release receives fixes.
