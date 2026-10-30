//! One handler module per command group, plus the shared `Ctx` and the
//! ID/name resolution helpers (guilds and channels accept either).

pub mod api;
pub mod app;
pub mod auth;
pub mod channels;
pub mod config;
pub mod delete;
pub mod guilds;
pub mod info;
pub mod me;
pub mod messages;
pub mod send;

use pk_cli_config::ConfigStore;
use pk_cli_core::{output, CliError};
use pk_cli_secrets::CredentialStore;
use serde_json::Value;

use crate::cli::{Cli, BIN};
use crate::client::Discord;
use crate::config::Config;

/// Keychain account name under the family service `piekstra.discord`.
pub const ACCOUNT: &str = "bot-token";

pub struct Ctx<'a> {
    pub cli: &'a Cli,
    pub store: ConfigStore,
    pub creds: CredentialStore,
}

impl<'a> Ctx<'a> {
    pub fn new(cli: &'a Cli) -> Self {
        Ctx {
            cli,
            store: ConfigStore::new(BIN),
            creds: CredentialStore::for_binary(BIN),
        }
    }

    pub fn config(&self) -> Result<Config, CliError> {
        self.store.load()
    }

    /// An authenticated client from the keychain-stored bot token.
    pub fn connect(&self) -> Result<Discord, CliError> {
        let token = self.creds.get(ACCOUNT)?.ok_or_else(|| {
            CliError::Auth("no bot token stored — run `discord auth login`".into())
        })?;
        Discord::new(token)
    }

    /// Emit per the output contract: the DTO in `--json` mode, the custom
    /// text rendering otherwise.
    pub fn emit(&self, dto: &Value, text: impl FnOnce(&Value)) {
        if self.cli.common.json {
            output::json(dto);
        } else {
            text(dto);
        }
    }

    /// Resolve a guild argument (ID or name) to a guild ID, falling back to
    /// the configured default guild when no argument is given.
    pub fn resolve_guild(&self, dc: &Discord, arg: Option<&str>) -> Result<String, CliError> {
        let cfg_guild = self.config()?.guild;
        let arg = arg
            .map(str::to_string)
            .or(cfg_guild)
            .ok_or_else(|| CliError::Usage(
                "no guild given — pass a guild ID/name or set a default with `discord config set guild <GUILD>`".into(),
            ))?;
        if is_snowflake(&arg) {
            return Ok(arg);
        }
        let guilds = dc.guilds()?;
        let matched = find_by_name(guilds.as_array().unwrap_or(&vec![]), &arg, "guild")?;
        Ok(matched)
    }

    /// Resolve a channel argument: a snowflake passes through; a `#name`
    /// is looked up within `--guild` / the configured guild.
    pub fn resolve_channel(
        &self,
        dc: &Discord,
        channel: &str,
        guild: Option<&str>,
    ) -> Result<String, CliError> {
        let name = channel.strip_prefix('#').unwrap_or(channel);
        if is_snowflake(name) {
            return Ok(name.to_string());
        }
        let guild_id = self.resolve_guild(dc, guild)?;
        let channels = dc.guild_channels(&guild_id)?;
        find_channel_by_name(channels.as_array().unwrap_or(&vec![]), name)
    }
}

/// Text-messageable channel types (text, announcement, threads, forum, media).
/// `messages`/`send` target these; a same-named voice channel shouldn't create
/// ambiguity when a single text channel matches.
fn is_text_channel(v: &Value) -> bool {
    matches!(
        v.get("type").and_then(Value::as_i64),
        Some(0 | 5 | 10 | 11 | 12 | 15 | 16)
    )
}

/// Resolve a channel by name, preferring text-messageable channels: if the
/// name collides across types (e.g. text `general` and voice `General`) but
/// exactly one match is a text channel, pick it; otherwise fall back to the
/// generic ambiguity handling.
pub fn find_channel_by_name(channels: &[Value], name: &str) -> Result<String, CliError> {
    let wanted = name.to_lowercase();
    let matches: Vec<&Value> = channels
        .iter()
        .filter(|v| {
            v.get("name")
                .and_then(Value::as_str)
                .is_some_and(|n| n.to_lowercase() == wanted)
        })
        .collect();
    if matches.len() > 1 {
        let text: Vec<&Value> = matches
            .iter()
            .copied()
            .filter(|v| is_text_channel(v))
            .collect();
        if let [one] = text.as_slice() {
            return Ok(one
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string());
        }
    }
    find_by_name(channels, name, "channel")
}

/// Discord IDs (snowflakes) are all-digit strings.
pub fn is_snowflake(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

/// Find exactly one item whose `name` matches (case-insensitive) and return
/// its `id`. Zero matches → not found; several → usage error listing IDs.
pub fn find_by_name(items: &[Value], name: &str, kind: &str) -> Result<String, CliError> {
    let wanted = name.to_lowercase();
    let matches: Vec<&Value> = items
        .iter()
        .filter(|v| {
            v.get("name")
                .and_then(Value::as_str)
                .is_some_and(|n| n.to_lowercase() == wanted)
        })
        .collect();
    match matches.as_slice() {
        [] => Err(CliError::NotFound(format!("no {kind} named `{name}`"))),
        [one] => Ok(one
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()),
        many => {
            let ids: Vec<&str> = many
                .iter()
                .filter_map(|v| v.get("id").and_then(Value::as_str))
                .collect();
            Err(CliError::Usage(format!(
                "{} {kind}s are named `{name}` — use an ID instead: {}",
                many.len(),
                ids.join(", ")
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn snowflakes_are_all_digits() {
        assert!(is_snowflake("1394201032425340938"));
        assert!(!is_snowflake("general"));
        assert!(!is_snowflake("123abc"));
        assert!(!is_snowflake(""));
    }

    #[test]
    fn find_by_name_matches_case_insensitively() {
        let items = vec![
            json!({"id": "1", "name": "General"}),
            json!({"id": "2", "name": "random"}),
        ];
        assert_eq!(find_by_name(&items, "general", "channel").unwrap(), "1");
        assert!(matches!(
            find_by_name(&items, "nope", "channel"),
            Err(CliError::NotFound(_))
        ));
    }

    #[test]
    fn find_by_name_rejects_ambiguity() {
        let items = vec![
            json!({"id": "1", "name": "general"}),
            json!({"id": "2", "name": "general"}),
        ];
        let err = find_by_name(&items, "general", "channel").unwrap_err();
        assert!(matches!(err, CliError::Usage(_)));
        assert!(err.to_string().contains("1, 2"));
    }

    #[test]
    fn find_channel_prefers_text_over_voice_on_name_collision() {
        // text `general` (type 0) vs voice `General` (type 2) → pick the text one.
        let channels = vec![
            json!({"id": "10", "name": "general", "type": 0}),
            json!({"id": "20", "name": "General", "type": 2}),
        ];
        assert_eq!(find_channel_by_name(&channels, "general").unwrap(), "10");

        // Two text channels with the same name stay ambiguous.
        let two_text = vec![
            json!({"id": "1", "name": "general", "type": 0}),
            json!({"id": "2", "name": "general", "type": 0}),
        ];
        assert!(matches!(
            find_channel_by_name(&two_text, "general"),
            Err(CliError::Usage(_))
        ));
    }
}
