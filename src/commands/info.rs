//! `discord info` — machine-readable capability discovery (`cli-info/v1`).

use pk_cli_core::info::{AuthInfo, CliInfo};
use pk_cli_core::{output, CliError};

use crate::cli::{BIN, REPO};

pub fn run() -> Result<(), CliError> {
    let info = CliInfo::new(
        BIN,
        env!("CARGO_PKG_VERSION"),
        &format!("https://github.com/{REPO}"),
        AuthInfo {
            required: true,
            // SPEC v1 method vocabulary; the credential is a Discord bot token.
            method: "password".into(),
            login_hint: Some(format!("{BIN} auth login")),
        },
        &[
            "me", "guilds", "channels", "messages", "send", "delete", "app", "api",
        ],
    );
    output::json(&serde_json::to_value(&info).unwrap_or_default());
    Ok(())
}
