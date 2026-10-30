//! `discord channels` — the channels of a guild (`channels/v1`).

use pk_cli_core::{output, CliError};
use serde_json::{json, Value};

use crate::commands::Ctx;

pub fn run(ctx: &Ctx, guild: Option<&str>) -> Result<(), CliError> {
    let dc = ctx.connect()?;
    let guild_id = ctx.resolve_guild(&dc, guild)?;
    let raw = dc.guild_channels(&guild_id)?;
    let mut channels: Vec<Value> = raw
        .as_array()
        .map(|arr| arr.iter().map(row).collect())
        .unwrap_or_default();
    channels.sort_by(|a, b| {
        let key = |v: &Value| {
            (
                v["type"].as_str().unwrap_or("").to_string(),
                v["position"].as_i64().unwrap_or(0),
            )
        };
        key(a).cmp(&key(b))
    });
    let dto = json!({
        "schema": "channels/v1",
        "guild_id": guild_id,
        "count": channels.len(),
        "channels": channels,
    });
    ctx.emit(&dto, |v| {
        output::table(v["channels"].as_array().map_or(&[], Vec::as_slice));
    });
    Ok(())
}

fn row(c: &Value) -> Value {
    let mut r = json!({
        "id": c.get("id"),
        "name": c.get("name"),
        "type": type_name(c.get("type").and_then(Value::as_i64).unwrap_or(-1)),
        "position": c.get("position"),
    });
    if let Some(topic) = c.get("topic").and_then(Value::as_str) {
        if !topic.is_empty() {
            r["topic"] = json!(topic);
        }
    }
    r
}

/// Human name for Discord's numeric channel type.
fn type_name(t: i64) -> &'static str {
    match t {
        0 => "text",
        1 => "dm",
        2 => "voice",
        3 => "group-dm",
        4 => "category",
        5 => "announcement",
        10 => "announcement-thread",
        11 => "thread",
        12 => "private-thread",
        13 => "stage",
        14 => "directory",
        15 => "forum",
        16 => "media",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_channel_types_map_to_names() {
        assert_eq!(type_name(0), "text");
        assert_eq!(type_name(2), "voice");
        assert_eq!(type_name(4), "category");
        assert_eq!(type_name(15), "forum");
        assert_eq!(type_name(99), "unknown");
    }
}
