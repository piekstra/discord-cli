//! `discord messages` — recent messages from a channel (`messages/v1`).
//!
//! Discord returns newest-first; we reverse so text output reads like a chat
//! log. Content arrives empty unless the app has the **Message Content**
//! privileged intent enabled (developer portal → Bot → intents).

use pk_cli_core::CliError;
use serde_json::{json, Value};

use crate::cli::MessagesArgs;
use crate::commands::Ctx;

pub fn run(ctx: &Ctx, args: &MessagesArgs) -> Result<(), CliError> {
    let dc = ctx.connect()?;
    let channel_id = ctx.resolve_channel(&dc, &args.channel, args.guild.as_deref())?;
    let raw = dc.channel_messages(
        &channel_id,
        args.limit,
        args.before.as_deref(),
        args.after.as_deref(),
    )?;
    let mut messages: Vec<Value> = raw
        .as_array()
        .map(|arr| arr.iter().map(row).collect())
        .unwrap_or_default();
    messages.reverse(); // chat order: oldest first
    let dto = json!({
        "schema": "messages/v1",
        "channel_id": channel_id,
        "count": messages.len(),
        "messages": messages,
    });
    ctx.emit(&dto, render);
    Ok(())
}

fn row(m: &Value) -> Value {
    let mut r = json!({
        "id": m.get("id"),
        "timestamp": m.get("timestamp"),
        "author": m.pointer("/author/username"),
        "author_id": m.pointer("/author/id"),
        "content": m.get("content"),
    });
    let attachments = m
        .get("attachments")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    if attachments > 0 {
        r["attachments"] = json!(attachments);
    }
    if let Some(reply) = m.pointer("/message_reference/message_id") {
        r["reply_to"] = reply.clone();
    }
    r
}

/// Chat-log text rendering: `[time] author: content`, continuation lines
/// indented, message ID at hand for replies.
fn render(v: &Value) {
    let Some(messages) = v["messages"].as_array() else {
        return;
    };
    if messages.is_empty() {
        println!("(no messages)");
        return;
    }
    for m in messages {
        let ts = compact_timestamp(m["timestamp"].as_str().unwrap_or(""));
        let author = m["author"].as_str().unwrap_or("?");
        let id = m["id"].as_str().unwrap_or("");
        let mut suffix = String::new();
        if let Some(n) = m["attachments"].as_u64() {
            suffix.push_str(&format!(
                " [{n} attachment{}]",
                if n == 1 { "" } else { "s" }
            ));
        }
        if m.get("reply_to").is_some() {
            suffix.push_str(" [reply]");
        }
        let content = m["content"].as_str().unwrap_or("");
        let mut lines = content.lines();
        let first = lines.next().unwrap_or("");
        println!("[{ts}] {author} ({id}): {first}{suffix}");
        for line in lines {
            println!("    {line}");
        }
    }
}

/// `2026-07-15T17:20:11.123000+00:00` → `2026-07-15 17:20` (UTC).
fn compact_timestamp(ts: &str) -> String {
    match ts.get(..16) {
        Some(t) => t.replace('T', " "),
        None => ts.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_compact_to_minutes() {
        assert_eq!(
            compact_timestamp("2026-07-15T17:20:11.123000+00:00"),
            "2026-07-15 17:20"
        );
        assert_eq!(compact_timestamp(""), "");
    }

    #[test]
    fn row_flattens_author_and_marks_replies() {
        let m = json!({
            "id": "3",
            "timestamp": "2026-07-15T17:20:11+00:00",
            "author": {"id": "9", "username": "caleb"},
            "content": "hi",
            "attachments": [{"id": "a"}],
            "message_reference": {"message_id": "2"},
        });
        let r = row(&m);
        assert_eq!(r["author"], "caleb");
        assert_eq!(r["author_id"], "9");
        assert_eq!(r["attachments"], 1);
        assert_eq!(r["reply_to"], "2");
    }
}
