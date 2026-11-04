//! `discord config` — non-secret settings at `~/.config/discord/config.json`.

use pk_cli_core::{output, CliError};

use crate::cli::ConfigCmd;
use crate::commands::Ctx;
use crate::config::{Config, KEYS};

pub fn run(ctx: &Ctx, cmd: &ConfigCmd) -> Result<(), CliError> {
    match cmd {
        ConfigCmd::Path => {
            println!("{}", ctx.store.path()?.display());
            Ok(())
        }
        ConfigCmd::Show => {
            let cfg: Config = ctx.config()?;
            let v = serde_json::to_value(&cfg).unwrap_or_default();
            if ctx.cli.common.json {
                output::json(&v);
            } else {
                output::render(&v);
            }
            Ok(())
        }
        ConfigCmd::Set { key, value } => {
            let mut cfg: Config = ctx.config()?;
            apply(&mut cfg, key, Some(value))?;
            ctx.store.save(&cfg)
        }
        ConfigCmd::Unset { key } => {
            let mut cfg: Config = ctx.config()?;
            apply(&mut cfg, key, None)?;
            ctx.store.save(&cfg)
        }
    }
}

fn apply(cfg: &mut Config, key: &str, value: Option<&str>) -> Result<(), CliError> {
    if key == "secret_sources" {
        cfg.secret_sources = value
            .map(|text| {
                text.parse()
                    .and_then(crate::config::usable_order)
                    .map_err(CliError::Usage)
            })
            .transpose()?;
        return Ok(());
    }
    let value = value.map(str::to_string);
    match key {
        "guild" => cfg.guild = value,
        "username" => cfg.username = value,
        "op-item" => cfg.op_item = value,
        other => {
            return Err(CliError::Usage(format!(
                "unknown config key `{other}` (known: {})",
                KEYS.join(", ")
            )))
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_keys_apply_and_unknown_fail() {
        let mut cfg = Config::default();
        apply(&mut cfg, "guild", Some("my-server")).unwrap();
        assert_eq!(cfg.guild.as_deref(), Some("my-server"));
        apply(&mut cfg, "guild", None).unwrap();
        assert!(cfg.guild.is_none());
        assert!(apply(&mut cfg, "nope", Some("x")).is_err());

        apply(&mut cfg, "secret_sources", Some("keychain")).unwrap();
        assert_eq!(cfg.secret_sources, Some("keychain".parse().unwrap()));
        apply(&mut cfg, "secret_sources", None).unwrap();
        assert!(cfg.secret_sources.is_none());
        for refused in ["nowhere", "op", "op,keychain"] {
            assert_eq!(
                apply(&mut cfg, "secret_sources", Some(refused))
                    .unwrap_err()
                    .exit_code(),
                2,
                "{refused}"
            );
        }
        assert!(cfg.secret_sources.is_none());
    }
}
