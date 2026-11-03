# Changelog

All notable changes to `discord` are documented here. Newest first. The project follows
[SemVer](https://semver.org/); until 1.0, minors carry the breaking-change signal.

## 0.2.0 — 2026-11-03

- `discord mcp`: three read-only tools for Claude: `discord_servers`, `discord_channels` and
  `discord_messages`. No tool can send, edit, delete or moderate anything. They run over stdio
  for Claude Code and Claude Desktop, and with `--http` on 127.0.0.1 behind a bearer token for
  Claude on the web and mobile through your own Cloudflare Worker and tunnel.
- `auth logout --forget` also removes the connector token of `mcp --http`.
- Releases attach `discord-<version>.mcpb`, a Claude Desktop extension, and a Windows x86_64
  tarball, and publish only once every asset is there.
- `self-update` says to restart a running `discord mcp --http` and to install the new `.mcpb`.
- The config follows `USERPROFILE` on Windows.
- cli-common v0.28.0, up from v0.24.0.
