//! `discord app` — application management.
//!
//! `get` / `intents` / `invite-url` ride the official API with the stored
//! bot token. `list` / `create` / `reset-token` are developer-portal
//! operations with no bot API: they open an interactive browser via the
//! Playwright helper (`src/portal.rs`, `docs/api.md`).

use pk_cli_core::{output, CliError};
use pk_cli_secrets::Secret;
use serde_json::{json, Value};

use crate::cli::{AppCmd, IntentsCmd, PortalArgs};
use crate::client::Discord;
use crate::commands::{Ctx, ACCOUNT};
use crate::portal;

/// Privileged gateway intents as (name, full bit, limited bit). Unverified
/// apps (< 100 servers) may set only the `*_LIMITED` bits via the API; the
/// full bits are granted by Discord during verification.
const INTENTS: &[(&str, u64, u64)] = &[
    ("presence", 1 << 12, 1 << 13),
    ("guild-members", 1 << 14, 1 << 15),
    ("message-content", 1 << 18, 1 << 19),
];

pub fn run(ctx: &Ctx, cmd: &AppCmd) -> Result<(), CliError> {
    match cmd {
        AppCmd::Get => get(ctx),
        AppCmd::Intents(cmd) => intents(ctx, cmd),
        AppCmd::InviteUrl { permissions } => invite_url(ctx, *permissions),
        AppCmd::List(portal) => list(ctx, portal),
        AppCmd::Create { name, portal } => create(ctx, name, portal),
        AppCmd::ResetToken { app_id, portal } => reset_token(ctx, app_id.as_deref(), portal),
        AppCmd::Logout => portal_logout(ctx),
    }
}

fn get(ctx: &Ctx) -> Result<(), CliError> {
    let dc = ctx.connect()?;
    let app = dc.application()?;
    let flags = app["flags"].as_u64().unwrap_or(0);
    let dto = json!({
        "schema": "app/v1",
        "id": app.get("id"),
        "name": app.get("name"),
        "description": app.get("description"),
        "bot_public": app.get("bot_public"),
        "flags": flags,
        "intents": intents_map(flags),
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

fn intents(ctx: &Ctx, cmd: &IntentsCmd) -> Result<(), CliError> {
    let dc = ctx.connect()?;
    let flags = match cmd {
        IntentsCmd::Show => dc.application()?["flags"].as_u64().unwrap_or(0),
        IntentsCmd::Enable { intent } | IntentsCmd::Disable { intent } => {
            let enable = matches!(cmd, IntentsCmd::Enable { .. });
            let (name, full, limited) = lookup_intent(intent)?;
            let current = dc.application()?["flags"].as_u64().unwrap_or(0);
            if !enable && current & full != 0 {
                return Err(CliError::Upstream(format!(
                    "`{name}` is set via the verified (100+ servers) flag, which \
                     only the developer portal can change"
                )));
            }
            let wanted = if enable {
                current | limited
            } else {
                current & !limited
            };
            if wanted == current {
                if !ctx.cli.common.quiet {
                    eprintln!(
                        "`{name}` already {}",
                        if enable { "enabled" } else { "disabled" }
                    );
                }
                current
            } else {
                let updated = dc.update_application_flags(wanted)?;
                if !ctx.cli.common.quiet {
                    eprintln!("`{name}` {}", if enable { "enabled" } else { "disabled" });
                }
                updated["flags"].as_u64().unwrap_or(wanted)
            }
        }
    };
    let dto = json!({
        "schema": "intents/v1",
        "flags": flags,
        "intents": intents_map(flags),
    });
    ctx.emit(&dto, |v| output::kv(&v["intents"], 0));
    Ok(())
}

fn invite_url(ctx: &Ctx, permissions: u64) -> Result<(), CliError> {
    let dc = ctx.connect()?;
    let app = dc.application()?;
    let id = app["id"]
        .as_str()
        .ok_or_else(|| CliError::Upstream("application response is missing `id`".into()))?;
    let url = format!(
        "https://discord.com/oauth2/authorize?client_id={id}&scope=bot&permissions={permissions}"
    );
    let dto = json!({
        "schema": "invite-url/v1",
        "client_id": id,
        "permissions": permissions,
        "url": url,
    });
    ctx.emit(&dto, |v| {
        println!("{}", v["url"].as_str().unwrap_or_default())
    });
    Ok(())
}

/// The 1Password item **name** for portal auto-login: `--op-item` wins, then
/// the `op-item` config key. The helper reads it only if it hits the login
/// form, so an already-logged-in session never prompts.
fn portal_op_item(ctx: &Ctx, portal: &PortalArgs) -> Result<Option<String>, CliError> {
    Ok(portal.op_item.clone().or(ctx.config()?.op_item))
}

fn list(ctx: &Ctx, portal_args: &PortalArgs) -> Result<(), CliError> {
    let op_item = portal_op_item(ctx, portal_args)?;
    let raw = portal::run_op("list", &[], op_item.as_deref(), ctx.cli.common.quiet)?;
    let apps: Vec<Value> = raw
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|a| {
                    json!({
                        "id": a.get("id"),
                        "name": a.get("name"),
                        "bot_public": a.get("bot_public"),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let dto = json!({
        "schema": "apps/v1",
        "count": apps.len(),
        "apps": apps,
    });
    ctx.emit(&dto, |v| {
        output::table(v["apps"].as_array().map_or(&[], Vec::as_slice));
    });
    Ok(())
}

fn create(ctx: &Ctx, name: &str, portal_args: &PortalArgs) -> Result<(), CliError> {
    let op_item = portal_op_item(ctx, portal_args)?;
    let raw = portal::run_op("create", &[name], op_item.as_deref(), ctx.cli.common.quiet)?;
    let id = raw["id"].as_str().unwrap_or_default().to_string();
    let dto = json!({
        "schema": "app/v1",
        "id": id,
        "name": raw.get("name"),
    });
    if ctx.cli.common.json {
        output::json(&dto);
    } else if !ctx.cli.common.quiet {
        eprintln!("created application `{name}` ({id})");
        eprintln!("next: `discord app reset-token {id}` mints its bot token into the keychain");
    }
    Ok(())
}

fn reset_token(ctx: &Ctx, app_id: Option<&str>, portal_args: &PortalArgs) -> Result<(), CliError> {
    let app_id = match app_id {
        Some(id) => id.to_string(),
        None => {
            let dc = ctx.connect().map_err(|_| {
                CliError::Usage(
                    "no application ID given and no stored bot token to look it up — \
                     pass the ID (see `discord app list`)"
                        .into(),
                )
            })?;
            dc.application()?["id"]
                .as_str()
                .ok_or_else(|| CliError::Upstream("application response is missing `id`".into()))?
                .to_string()
        }
    };

    let op_item = portal_op_item(ctx, portal_args)?;
    let raw = portal::run_op(
        "reset-token",
        &[&app_id],
        op_item.as_deref(),
        ctx.cli.common.quiet,
    )?;
    let token = raw["token"]
        .as_str()
        .filter(|t| !t.is_empty())
        .ok_or_else(|| CliError::Upstream("portal helper returned no token".into()))?;

    // Verify the fresh token, then commit it to the keychain.
    let dc = Discord::new(Secret::new(token))?;
    let me = dc.me()?;
    ctx.creds.set(ACCOUNT, &Secret::new(token))?;
    let mut cfg = ctx.config()?;
    cfg.username = me["username"].as_str().map(str::to_string);
    ctx.store.save(&cfg)?;

    let dto = json!({
        "schema": "auth-status/v1",
        "required": true,
        "authenticated": true,
        "method": "password",
        "username": cfg.username,
        "credential_in_keychain": true,
    });
    if ctx.cli.common.json {
        output::json(&dto);
    } else if !ctx.cli.common.quiet {
        eprintln!(
            "new bot token stored in the OS keychain (verified as {})",
            cfg.username.as_deref().unwrap_or("?")
        );
    }
    Ok(())
}

fn portal_logout(ctx: &Ctx) -> Result<(), CliError> {
    let profile = portal::profile_dir()?;
    if profile.exists() {
        std::fs::remove_dir_all(&profile)
            .map_err(|e| CliError::Other(format!("removing {}: {e}", profile.display())))?;
        if !ctx.cli.common.quiet {
            eprintln!("portal browser session forgotten ({})", profile.display());
        }
    } else if !ctx.cli.common.quiet {
        eprintln!("no portal browser session to forget");
    }
    Ok(())
}

fn lookup_intent(name: &str) -> Result<(&'static str, u64, u64), CliError> {
    INTENTS
        .iter()
        .find(|(n, _, _)| *n == name)
        .copied()
        .ok_or_else(|| {
            let known: Vec<&str> = INTENTS.iter().map(|(n, _, _)| *n).collect();
            CliError::Usage(format!(
                "unknown intent `{name}` (known: {})",
                known.join(", ")
            ))
        })
}

/// `{intent name: enabled}` — enabled means either the full or limited bit.
fn intents_map(flags: u64) -> Value {
    let mut m = serde_json::Map::new();
    for (name, full, limited) in INTENTS {
        m.insert(name.to_string(), json!(flags & (full | limited) != 0));
    }
    Value::Object(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intent_lookup_and_flag_math() {
        let (_, full, limited) = lookup_intent("message-content").unwrap();
        assert_eq!(full, 1 << 18);
        assert_eq!(limited, 1 << 19);
        assert!(lookup_intent("nope").is_err());

        let flags = limited;
        assert_eq!(intents_map(flags)["message-content"], true);
        assert_eq!(intents_map(flags)["presence"], false);
        assert_eq!(intents_map(1 << 18)["message-content"], true); // full bit counts too
        assert_eq!(intents_map(flags & !limited)["message-content"], false);
    }
}
