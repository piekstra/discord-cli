//! Blocking client for the Discord REST API (v10) using bot-token auth.
//!
//! Endpoint notes live in `docs/api.md`. Every response is mapped onto the
//! family exit-code contract; Discord's JSON error body (`message` + `code`)
//! is folded into the error text so failures are self-explanatory.

use std::time::Duration;

use pk_cli_core::CliError;
use pk_cli_secrets::Secret;
use serde_json::{json, Value};

use crate::cli::{BIN, REPO};

pub const API_BASE: &str = "https://discord.com/api/v10";

/// 429s with a short Retry-After are waited out this many times before we
/// give up and surface the rate limit as an upstream error.
const RATE_LIMIT_RETRIES: u32 = 2;
const RATE_LIMIT_MAX_WAIT_SECS: f64 = 5.0;

pub struct Discord {
    http: reqwest::blocking::Client,
    token: Secret,
}

impl Discord {
    pub fn new(token: Secret) -> Result<Self, CliError> {
        if token.is_empty() {
            return Err(CliError::Auth(
                "empty bot token — run `discord auth login`".into(),
            ));
        }
        // Discord asks bots for a `DiscordBot ($url, $version)` user agent.
        let http = pk_cli_http::builder(BIN, env!("CARGO_PKG_VERSION"))
            .user_agent(format!(
                "DiscordBot (https://github.com/{REPO}, {})",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .map_err(|e| CliError::Other(format!("failed to build HTTP client: {e}")))?;
        Ok(Discord { http, token })
    }

    /// One authenticated request. Waits out brief rate limits, then maps the
    /// response onto the exit-code contract and parses JSON (204 → null).
    pub fn request(
        &self,
        method: reqwest::Method,
        url: &str,
        body: Option<&Value>,
    ) -> Result<Value, CliError> {
        let mut attempt = 0;
        loop {
            let mut req = self
                .http
                .request(method.clone(), url)
                .header("Authorization", format!("Bot {}", self.token.expose()));
            if let Some(b) = body {
                req = req.json(b);
            }
            let resp = req.send()?;
            if resp.status().as_u16() == 429 && attempt < RATE_LIMIT_RETRIES {
                let wait = retry_after_secs(&resp).unwrap_or(1.0);
                if wait <= RATE_LIMIT_MAX_WAIT_SECS {
                    std::thread::sleep(Duration::from_secs_f64(wait));
                    attempt += 1;
                    continue;
                }
            }
            return into_value(resp);
        }
    }

    fn get(&self, path: &str) -> Result<Value, CliError> {
        self.request(reqwest::Method::GET, &format!("{API_BASE}{path}"), None)
    }

    /// The bot's own user (also serves as the cheap token verification).
    pub fn me(&self) -> Result<Value, CliError> {
        self.get("/users/@me")
    }

    /// Guilds the bot has joined (first page, up to 200).
    pub fn guilds(&self) -> Result<Value, CliError> {
        self.get("/users/@me/guilds")
    }

    pub fn guild_channels(&self, guild_id: &str) -> Result<Value, CliError> {
        self.get(&format!("/guilds/{guild_id}/channels"))
    }

    /// The application this bot token belongs to.
    pub fn application(&self) -> Result<Value, CliError> {
        self.get("/applications/@me")
    }

    /// Update the application's flags. Discord only lets the API change the
    /// `*_LIMITED` privileged-intent bits (apps under 100 servers).
    pub fn update_application_flags(&self, flags: u64) -> Result<Value, CliError> {
        self.request(
            reqwest::Method::PATCH,
            &format!("{API_BASE}/applications/@me"),
            Some(&json!({ "flags": flags })),
        )
    }

    pub fn channel_messages(
        &self,
        channel_id: &str,
        limit: u8,
        before: Option<&str>,
        after: Option<&str>,
    ) -> Result<Value, CliError> {
        let mut path = format!("/channels/{channel_id}/messages?limit={limit}");
        if let Some(b) = before {
            path.push_str(&format!("&before={b}"));
        }
        if let Some(a) = after {
            path.push_str(&format!("&after={a}"));
        }
        self.get(&path)
    }

    /// Delete a message from a channel (204 → null).
    pub fn delete_message(&self, channel_id: &str, message_id: &str) -> Result<Value, CliError> {
        self.request(
            reqwest::Method::DELETE,
            &format!("{API_BASE}/channels/{channel_id}/messages/{message_id}"),
            None,
        )
    }

    pub fn create_message(
        &self,
        channel_id: &str,
        content: &str,
        reply_to: Option<&str>,
    ) -> Result<Value, CliError> {
        let mut body = json!({ "content": content });
        if let Some(id) = reply_to {
            body["message_reference"] = json!({ "message_id": id });
        }
        self.request(
            reqwest::Method::POST,
            &format!("{API_BASE}/channels/{channel_id}/messages"),
            Some(&body),
        )
    }
}

fn retry_after_secs(resp: &reqwest::blocking::Response) -> Option<f64> {
    resp.headers()
        .get("Retry-After")?
        .to_str()
        .ok()?
        .parse()
        .ok()
}

/// Map a response onto the exit-code contract: 401/403 → auth (3), 404 → not
/// found (4), other non-2xx → upstream (5); then parse JSON (204 → null).
fn into_value(resp: reqwest::blocking::Response) -> Result<Value, CliError> {
    let status = resp.status();
    if status.is_success() {
        if status.as_u16() == 204 {
            return Ok(Value::Null);
        }
        return resp
            .json::<Value>()
            .map_err(|e| CliError::Upstream(format!("parsing Discord JSON: {e}")));
    }
    let detail = error_detail(resp.json::<Value>().ok());
    match status.as_u16() {
        401 => Err(CliError::Auth(format!(
            "Discord rejected the bot token (HTTP 401{detail}) — run `discord auth login` again"
        ))),
        403 => Err(CliError::Auth(format!(
            "the bot lacks access or permissions (HTTP 403{detail})"
        ))),
        404 => Err(CliError::NotFound(format!("HTTP 404 from Discord{detail}"))),
        429 => Err(CliError::Upstream(format!(
            "rate limited by Discord (HTTP 429{detail})"
        ))),
        s => Err(CliError::Upstream(format!(
            "Discord returned HTTP {s}{detail}"
        ))),
    }
}

/// `: message (code N)` from Discord's error body, or "" when unavailable.
fn error_detail(body: Option<Value>) -> String {
    let Some(body) = body else {
        return String::new();
    };
    let message = body.get("message").and_then(Value::as_str).unwrap_or("");
    if message.is_empty() {
        return String::new();
    }
    match body.get("code").and_then(Value::as_i64) {
        Some(code) if code != 0 => format!(": {message} (code {code})"),
        _ => format!(": {message}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_detail_folds_message_and_code() {
        assert_eq!(
            error_detail(Some(json!({"message": "Missing Access", "code": 50001}))),
            ": Missing Access (code 50001)"
        );
        assert_eq!(
            error_detail(Some(json!({"message": "401: Unauthorized", "code": 0}))),
            ": 401: Unauthorized"
        );
        assert_eq!(error_detail(Some(json!({"retry_after": 1.2}))), "");
        assert_eq!(error_detail(None), "");
    }

    #[test]
    fn empty_token_is_rejected() {
        assert!(matches!(
            Discord::new(Secret::new("")),
            Err(CliError::Auth(_))
        ));
    }
}
