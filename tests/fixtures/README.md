# Test fixtures

Payload shapes for the `src/mcp.rs` tests. Every value is invented: no server, channel,
person, message, id or date here is real. `src/mcp/tests.rs` holds the identifying fields to
dummy shapes.

| File | Shape of | Handling |
|---|---|---|
| `guilds.json` | `GET /users/@me/guilds` | Two invented servers, trimmed. |
| `channels.json` | `GET /guilds/{id}/channels` | Invented channels of four kinds, out of order on purpose. |
| `messages.json` | `GET /channels/{id}/messages` | Three invented messages, newest first: one steering text, a reply and an attachment whose signed link is a dummy, and one with empty content. |
