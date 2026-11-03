//! Non-secret preferences at `~/.config/discord/config.json`. The bot token
//! never lands here — it lives in the OS keychain (see `pk-cli-secrets`).

use pk_cli_secrets::{SourceKind, SourceOrder};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    /// Default guild (ID or name) for resolving channels given by name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild: Option<String>,
    /// Bot username captured at `auth login` (display nicety, non-secret).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// 1Password item name for portal auto-login (the item reference only —
    /// the credential itself always stays in 1Password).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub op_item: Option<String>,
    /// Where `discord mcp` looks for the bot token, in order (`env,keychain`).
    /// `$DISCORD_SECRET_SOURCES` overrides it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_sources: Option<SourceOrder>,
}

pub const KEYS: &[&str] = &["guild", "username", "op-item", "secret_sources"];

/// An order this CLI can honor. The bot token has no 1Password reference (`op-item` is the
/// portal's sign-in), so an order naming `op` would read nothing there and say nothing about it.
pub fn usable_order(order: SourceOrder) -> Result<SourceOrder, String> {
    if order.sources().contains(&SourceKind::Op) {
        return Err(
            "the bot token is read from `env` and `keychain` only, so `op` cannot be in the order"
                .into(),
        );
    }
    Ok(order)
}
