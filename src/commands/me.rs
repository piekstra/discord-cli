//! `discord me` — the authenticated bot user (`user/v1`).

use pk_cli_core::{output, CliError};
use serde_json::{json, Value};

use crate::commands::Ctx;

pub fn run(ctx: &Ctx) -> Result<(), CliError> {
    let dc = ctx.connect()?;
    let me = dc.me()?;
    let dto = json!({
        "schema": "user/v1",
        "id": me.get("id"),
        "username": me.get("username"),
        "global_name": me.get("global_name"),
        "bot": me.get("bot").and_then(Value::as_bool).unwrap_or(false),
    });
    ctx.emit(&dto, |v| {
        let mut view = v.clone();
        if let Some(m) = view.as_object_mut() {
            m.remove("schema");
        }
        output::render(&view);
    });
    Ok(())
}
