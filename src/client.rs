//! Blocking client for the Discord REST API (v10) using bot-token auth.
//!
//! Endpoint notes live in `docs/api.md`. Every response is mapped onto the
//! family exit-code contract; Discord's JSON error body (`message` + `code`)
//! is folded into the error text so failures are self-explanatory.

use std::time::Duration;

use pk_cli_core::transport::secure_redirects;
use pk_cli_core::{CliError, Reason};
use pk_cli_secrets::Secret;
use serde_json::{json, Value};

use crate::cli::{BIN, REPO};

pub const API_BASE: &str = "https://discord.com/api/v10";

/// 429s with a short Retry-After are waited out this many times before we
/// give up and surface the rate limit as an upstream error.
const RATE_LIMIT_RETRIES: u32 = 2;
const RATE_LIMIT_MAX_WAIT_SECS: f64 = 5.0;
/// The most guilds Discord returns per request.
const GUILD_PAGE: usize = 200;

/// What a 403 says, so a tool can tell it from a token Discord refused: signing in again
/// does not fix it.
pub const NO_ACCESS: &str = "the bot lacks access or permissions";

pub struct Discord {
    http: reqwest::blocking::Client,
    token: Secret,
    base: String,
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
            .redirect(secure_redirects())
            .build()
            .map_err(|e| CliError::Other(format!("failed to build HTTP client: {e}")))?;
        Ok(Discord {
            http,
            token,
            base: API_BASE.to_string(),
        })
    }

    /// A client for a local stub. Tests only, so no release can send the token elsewhere.
    #[cfg(test)]
    pub fn at(base: &str, token: Secret) -> Result<Self, CliError> {
        let mut client = Self::new(token)?;
        client.base = base.to_string();
        Ok(client)
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
        self.request(reqwest::Method::GET, &format!("{}{path}", self.base), None)
    }

    /// The bot's own user (also serves as the cheap token verification).
    pub fn me(&self) -> Result<Value, CliError> {
        self.get("/users/@me")
    }

    /// Guilds the bot has joined (first page, up to 200).
    pub fn guilds(&self) -> Result<Value, CliError> {
        self.get("/users/@me/guilds")
    }

    /// Every guild the bot has joined, following Discord's pages.
    pub fn all_guilds(&self) -> Result<Value, CliError> {
        let mut all: Vec<Value> = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let path = match &after {
                Some(id) => format!("/users/@me/guilds?limit={GUILD_PAGE}&after={id}"),
                None => format!("/users/@me/guilds?limit={GUILD_PAGE}"),
            };
            let Value::Array(page) = self.get(&path)? else {
                return Err(CliError::Upstream(
                    "Discord did not answer with a list of guilds".into(),
                ));
            };
            let full = page.len() == GUILD_PAGE;
            all.extend(page);
            after = all
                .last()
                .and_then(|g| g.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string);
            if !full || after.is_none() {
                return Ok(Value::Array(all));
            }
        }
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
        401 => Err(
            CliError::Auth(format!("Discord rejected the bot token (HTTP 401{detail})"))
                .because(Reason::CredentialRefused)
                .with_remedy("run `discord auth login` again"),
        ),
        403 => {
            Err(CliError::Auth(format!("{NO_ACCESS} (HTTP 403{detail})"))
                .because(Reason::Forbidden))
        }
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

    /// A stub on 127.0.0.1. Answers each request with `respond(request)` as a raw
    /// HTTP response, and keeps each request so a test can see what arrived.
    fn stub(
        respond: impl Fn(&str) -> String + Send + 'static,
    ) -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = seen.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let mut raw = Vec::new();
                let mut buf = [0u8; 4096];
                let request = loop {
                    let n = stream.read(&mut buf).unwrap_or(0);
                    raw.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&raw).into_owned();
                    let complete = text.split_once("\r\n\r\n").is_some_and(|(head, body)| {
                        let length = head
                            .lines()
                            .find_map(|l| {
                                let (name, value) = l.split_once(':')?;
                                name.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse::<usize>().ok())?
                            })
                            .unwrap_or(0);
                        body.len() >= length
                    });
                    if n == 0 || complete {
                        break text;
                    }
                };
                let response = respond(&request);
                log.lock().unwrap().push(request);
                let _ = stream.write_all(response.as_bytes());
            }
        });
        (base, seen)
    }

    fn redirect_to(location: &str) -> String {
        format!("HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
    }

    #[test]
    fn a_redirect_that_leaves_https_is_refused() {
        let (base, seen) = stub(|_| redirect_to("http://www.example.invalid/next"));
        let err = Discord::new(Secret::new("dummy-token"))
            .unwrap()
            .request(reqwest::Method::GET, &format!("{base}/start"), None)
            .unwrap_err();
        assert!(err.to_string().contains("leaves https"), "{err}");
        assert_eq!(seen.lock().unwrap().len(), 1);
    }

    fn answered(status: u16) -> CliError {
        let (base, _) = stub(move |_| {
            let body = r#"{"message": "An example refusal", "code": 1}"#;
            format!(
                "HTTP/1.1 {status} Refused\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        });
        Discord::at(&base, Secret::new("dummy-token"))
            .unwrap()
            .me()
            .unwrap_err()
    }

    #[test]
    fn a_401_and_a_403_are_told_apart_by_type_and_both_exit_3() {
        let refused = answered(401);
        assert_eq!(refused.reason(), Some(Reason::CredentialRefused));
        assert!(refused.is_auth());
        assert_eq!(refused.remedy(), Some("run `discord auth login` again"));

        let forbidden = answered(403);
        assert_eq!(forbidden.reason(), Some(Reason::Forbidden));
        assert!(forbidden.is_auth());
        assert!(forbidden.message().starts_with(NO_ACCESS), "{forbidden}");
        assert!(
            forbidden.message().contains("An example refusal"),
            "{forbidden}"
        );
    }

    #[test]
    fn all_guilds_follows_the_pages_to_a_short_one() {
        let (base, seen) = stub(|request| {
            let rows: Vec<Value> = if request.contains("after=") {
                vec![json!({"id": "1200", "name": "Example"})]
            } else {
                (0..GUILD_PAGE)
                    .map(|i| json!({"id": (1000 + i).to_string(), "name": "Example"}))
                    .collect()
            };
            let body = Value::Array(rows).to_string();
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        });
        let got = Discord::at(&base, Secret::new("dummy-token"))
            .unwrap()
            .all_guilds()
            .unwrap();
        assert_eq!(got.as_array().unwrap().len(), GUILD_PAGE + 1);
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert!(
            seen[1].contains(&format!("after={}", 1000 + GUILD_PAGE - 1)),
            "{seen:?}"
        );
    }

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
