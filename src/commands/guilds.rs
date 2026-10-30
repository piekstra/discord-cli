//! `discord guilds` — the guilds (servers) the bot has joined (`guilds/v1`).

use pk_cli_core::{output, CliError};
use serde_json::{json, Value};

use crate::commands::Ctx;

pub fn run(ctx: &Ctx) -> Result<(), CliError> {
    let dc = ctx.connect()?;
    let raw = dc.guilds()?;
    let guilds: Vec<Value> = raw
        .as_array()
        .map(|arr| arr.iter().map(row).collect())
        .unwrap_or_default();
    let dto = json!({
        "schema": "guilds/v1",
        "count": guilds.len(),
        "guilds": guilds,
    });
    ctx.emit(&dto, |v| {
        output::table(v["guilds"].as_array().map_or(&[], Vec::as_slice));
    });
    Ok(())
}

fn row(g: &Value) -> Value {
    json!({
        "id": g.get("id"),
        "name": g.get("name"),
        "owner": g.get("owner").and_then(Value::as_bool).unwrap_or(false),
    })
}
