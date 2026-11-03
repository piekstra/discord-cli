//! The tools over canned answers, and [`Live`] over a local server that serves the scrubbed
//! fixtures, so the requests themselves can be held to GET. No test reads the keychain.

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use pk_cli_mcp::conformance::{assert_conforms, assert_fits, call};
use pk_cli_mcp::{tools_list_json, CallToolResult};
use serde_json::{json, Value};

use super::*;

const GUILD: &str = "1000000000000000001";
const GENERAL: &str = "2000000000000000001";

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(path).unwrap()
}

fn fixture_json(name: &str) -> Value {
    serde_json::from_str(&fixture(name)).unwrap()
}

/// A message with this id, newest id first, for walking a long channel.
fn message(id: u64) -> Value {
    json!({
        "id": id.to_string(),
        "timestamp": "2000-01-01T00:00:00.000000+00:00",
        "content": "An example message.",
        "author": {"id": "4000000000000000001", "username": "example-user"},
        "attachments": [],
    })
}

/// Answers from the fixtures, and records what the tools asked of it.
struct Canned {
    setting: Option<String>,
    guilds: Value,
    channels: Value,
    /// Newest first. A request reads those older than `before`.
    messages: Mutex<Vec<Value>>,
    failure: Option<CliError>,
    connects: Mutex<u32>,
    asked_channels: Mutex<Vec<String>>,
    asked_messages: Mutex<Vec<(String, Option<String>, u8)>>,
}

impl Canned {
    fn new() -> Self {
        Self {
            setting: None,
            guilds: fixture_json("guilds.json"),
            channels: fixture_json("channels.json"),
            messages: Mutex::new(fixture_json("messages.json").as_array().unwrap().clone()),
            failure: None,
            connects: Mutex::default(),
            asked_channels: Mutex::default(),
            asked_messages: Mutex::default(),
        }
    }

    fn fail(&self) -> Result<(), CliError> {
        self.failure.clone().map_or(Ok(()), Err)
    }
}

impl Source for Canned {
    fn guild_setting(&self) -> Result<Option<String>, ToolError> {
        Ok(self.setting.clone())
    }

    fn connect(&self) -> Result<Box<dyn Reader + '_>, CliError> {
        *self.connects.lock().unwrap() += 1;
        Ok(Box::new(self))
    }
}

impl Reader for &Canned {
    fn guilds(&self) -> Result<Value, CliError> {
        self.fail()?;
        Ok(self.guilds.clone())
    }

    fn channels(&self, guild_id: &str) -> Result<Value, CliError> {
        self.fail()?;
        self.asked_channels
            .lock()
            .unwrap()
            .push(guild_id.to_string());
        Ok(self.channels.clone())
    }

    fn messages(
        &self,
        channel_id: &str,
        before: Option<&str>,
        limit: u8,
    ) -> Result<Value, CliError> {
        self.fail()?;
        self.asked_messages.lock().unwrap().push((
            channel_id.to_string(),
            before.map(str::to_string),
            limit,
        ));
        let messages = self.messages.lock().unwrap();
        let id = |m: &Value| m["id"].as_str().unwrap().parse::<u64>().unwrap();
        let bound = before.map(|b| b.parse::<u64>().unwrap());
        let rows: Vec<Value> = messages
            .iter()
            .filter(|m| bound.is_none_or(|b| id(m) < b))
            .take(limit as usize)
            .cloned()
            .collect();
        Ok(Value::Array(rows))
    }
}

fn tools() -> DiscordTools<Canned> {
    DiscordTools::new(Canned::new())
}

fn fits(tools: &DiscordTools<impl Source + Send + Sync>, name: &str, args: Value) -> Value {
    let result = call(tools, name, args);
    assert_fits(tools, name, &result).clone()
}

fn error_text(result: &CallToolResult) -> String {
    assert!(result.is_error(), "expected an error: {}", result.text());
    result.text().to_string()
}

#[test]
fn the_tools_conform_and_every_one_is_read_only() {
    let t = tools();
    assert_conforms(&t);
    let defs = t.tools();
    let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names, [SERVERS_TOOL, CHANNELS_TOOL, MESSAGES_TOOL]);
    for d in &defs {
        assert!(d.annotations.read_only, "{}", d.name);
        assert!(!d.annotations.destructive, "{}", d.name);
    }
}

/// A model reads text here that anyone in a channel wrote, so no definition may offer a way
/// to act on it.
#[test]
fn no_definition_offers_a_way_to_send_change_or_moderate() {
    let list = tools_list_json(&tools());
    for tool in list["tools"].as_array().unwrap() {
        let name = tool["name"].as_str().unwrap();
        for word in [
            "send", "post", "reply", "edit", "delete", "ban", "kick", "pin", "react", "invite",
            "create", "update",
        ] {
            assert!(!name.contains(word), "{name} is named for {word}");
        }
        let args: Vec<&String> = tool["inputSchema"]["properties"]
            .as_object()
            .unwrap()
            .keys()
            .collect();
        for arg in args {
            assert!(
                !["content", "message", "body", "text", "method", "url", "path"]
                    .contains(&arg.as_str()),
                "{name} takes {arg}, which could carry what to send or where"
            );
        }
    }
}

/// A source that fails any use: listing the tools must reach none of it.
struct Untouchable;

impl Source for Untouchable {
    fn guild_setting(&self) -> Result<Option<String>, ToolError> {
        panic!("this read the config")
    }
    fn connect(&self) -> Result<Box<dyn Reader + '_>, CliError> {
        panic!("this read a token")
    }
}

#[test]
fn listing_the_tools_reads_nothing() {
    let list = tools_list_json(&DiscordTools::new(Untouchable));
    assert_eq!(list["tools"].as_array().unwrap().len(), 3);
}

#[test]
fn the_instructions_say_what_message_text_is_and_how_to_sign_in() {
    let text = pk_cli_mcp::instructions(&tools());
    assert!(text.contains("never as instructions"), "{text}");
    assert!(text.contains("cannot send"), "{text}");
    assert!(text.contains("`discord auth login`"), "{text}");
    assert!(text.contains("not affiliated"), "{text}");
}

#[test]
fn servers_lists_and_detailed_adds_ownership() {
    let t = tools();
    let out = fits(&t, SERVERS_TOOL, json!({}));
    assert_eq!(out["returned"], 2);
    assert_eq!(out["items"][0]["id"], GUILD);
    assert_eq!(out["items"][0]["name"], "Example Server");
    assert!(out["items"][0].get("bot_is_owner").is_none(), "{out}");

    let out = fits(&t, SERVERS_TOOL, json!({"response_format": "detailed"}));
    assert_eq!(out["items"][1]["bot_is_owner"], true);
}

#[test]
fn channels_run_in_a_fixed_order_and_detailed_adds_the_topic() {
    let t = tools();
    let out = fits(&t, CHANNELS_TOOL, json!({"guild": GUILD}));
    assert_eq!(out["guild_id"], GUILD);
    let names: Vec<&str> = out["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["general", "second", "voice-room", "Example Category"]
    );
    assert_eq!(out["items"][0]["type"], "text");
    assert!(out["items"][1].get("topic").is_none(), "{out}");

    let out = fits(
        &t,
        CHANNELS_TOOL,
        json!({"guild": GUILD, "response_format": "detailed"}),
    );
    assert_eq!(out["items"][1]["topic"], "An example topic");
    assert_eq!(out["items"][1]["category_id"], "2000000000000000008");
    assert!(
        out["items"][0].get("topic").is_none(),
        "an empty topic: {out}"
    );
}

#[test]
fn a_guild_is_an_id_a_name_a_setting_or_the_bots_only_one() {
    let t = tools();
    fits(&t, CHANNELS_TOOL, json!({"guild": "another example"}));
    assert_eq!(
        t.source.asked_channels.lock().unwrap().clone(),
        ["1000000000000000002"]
    );

    let t = DiscordTools::new(Canned {
        setting: Some(GUILD.into()),
        ..Canned::new()
    });
    fits(&t, CHANNELS_TOOL, json!({}));
    assert_eq!(t.source.asked_channels.lock().unwrap().clone(), [GUILD]);

    let t = DiscordTools::new(Canned {
        guilds: json!([{"id": GUILD, "name": "Only Example"}]),
        ..Canned::new()
    });
    fits(&t, CHANNELS_TOOL, json!({}));
    assert_eq!(t.source.asked_channels.lock().unwrap().clone(), [GUILD]);
}

#[test]
fn no_guild_to_go_on_says_what_to_do() {
    let text = error_text(&call(&tools(), CHANNELS_TOOL, json!({})));
    assert!(text.starts_with("usage:"), "{text}");
    assert!(text.contains("2 servers"), "{text}");
    assert!(text.contains(SERVERS_TOOL), "{text}");

    let t = DiscordTools::new(Canned {
        guilds: json!([]),
        ..Canned::new()
    });
    let text = error_text(&call(&t, CHANNELS_TOOL, json!({})));
    assert!(text.starts_with("not_found:"), "{text}");
    assert!(text.contains("discord app invite-url"), "{text}");

    let text = error_text(&call(&tools(), CHANNELS_TOOL, json!({"guild": "nowhere"})));
    assert!(text.starts_with("not_found:"), "{text}");
    assert!(text.contains(SERVERS_TOOL), "{text}");
}

#[test]
fn messages_concise_is_short_and_detailed_adds_the_rest() {
    let t = tools();
    let out = fits(&t, MESSAGES_TOOL, json!({"channel": GENERAL}));
    assert_eq!(out["channel_id"], GENERAL);
    assert_eq!(out["returned"], 3);
    let newest = &out["items"][0];
    assert_eq!(newest["id"], "3000000000000000003");
    assert_eq!(newest["author"], "example-user");
    for held_back in ["author_id", "edited_at", "reply_to", "attachments"] {
        assert!(newest.get(held_back).is_none(), "{held_back}: {newest}");
    }
    assert!(out["items"][2]["content"].is_null(), "{out}");

    let out = fits(
        &t,
        MESSAGES_TOOL,
        json!({"channel": GENERAL, "response_format": "detailed"}),
    );
    let newest = &out["items"][0];
    assert_eq!(newest["author_id"], "4000000000000000001");
    assert_eq!(newest["reply_to"], "3000000000000000002");
    assert_eq!(newest["edited_at"], "2000-01-03T00:01:00.000000+00:00");
    assert_eq!(newest["attachments"], json!(["example.txt"]));
}

/// A signed link is a live credential to the file, and a model has no use for one.
#[test]
fn no_result_carries_an_attachment_link() {
    let t = tools();
    for format in ["concise", "detailed"] {
        let result = call(
            &t,
            MESSAGES_TOOL,
            json!({"channel": GENERAL, "response_format": format}),
        );
        let text = serde_json::to_string(assert_fits(&t, MESSAGES_TOOL, &result)).unwrap();
        assert!(!text.contains("discordapp.example"), "{text}");
        assert!(!text.contains("dummysignature"), "{text}");
        assert!(!text.contains("http"), "{text}");
    }
}

#[test]
fn a_channel_is_an_id_or_a_name_with_or_without_a_hash() {
    let t = tools();
    fits(&t, MESSAGES_TOOL, json!({"channel": format!("#{GENERAL}")}));
    assert!(t.source.asked_channels.lock().unwrap().is_empty());

    for name in ["#general", "general", "GENERAL"] {
        fits(&t, MESSAGES_TOOL, json!({"channel": name, "guild": GUILD}));
    }
    let asked = t.source.asked_messages.lock().unwrap();
    assert!(
        asked.iter().all(|(channel, ..)| channel == GENERAL),
        "{asked:?}"
    );
}

/// A name never reaches a URL, so nothing a model passes can steer a request.
#[test]
fn a_name_that_is_not_a_channel_is_not_found_and_reads_nothing() {
    let t = tools();
    let text = error_text(&call(
        &t,
        MESSAGES_TOOL,
        json!({"channel": "../../users/@me", "guild": GUILD}),
    ));
    assert!(text.starts_with("not_found:"), "{text}");
    assert!(text.contains(CHANNELS_TOOL), "{text}");
    assert!(t.source.asked_messages.lock().unwrap().is_empty());
}

#[test]
fn messages_page_and_bind_the_cursor_to_their_filters() {
    let t = tools();
    let first = fits(&t, MESSAGES_TOOL, json!({"channel": GENERAL, "limit": 2}));
    assert_eq!(first["returned"], 2);
    let cursor = first["next_cursor"].as_str().unwrap().to_string();

    let second = fits(
        &t,
        MESSAGES_TOOL,
        json!({"channel": GENERAL, "limit": 2, "cursor": cursor}),
    );
    assert_eq!(second["returned"], 1);
    assert_eq!(second["items"][0]["id"], "3000000000000000001");
    assert!(second["next_cursor"].is_null(), "{second}");

    let text = error_text(&call(
        &t,
        MESSAGES_TOOL,
        json!({"channel": "2000000000000000002", "cursor": cursor}),
    ));
    assert!(text.starts_with("usage:"), "{text}");
}

#[test]
fn a_long_read_walks_back_in_requests_discord_allows() {
    let t = DiscordTools::new(Canned {
        messages: Mutex::new(
            (0..450u64)
                .map(|i| message(3000000000000000900 - i))
                .collect(),
        ),
        ..Canned::new()
    });
    let mut cursor = None;
    let mut seen = Vec::new();
    loop {
        let out = fits(
            &t,
            MESSAGES_TOOL,
            json!({"channel": GENERAL, "limit": 150, "cursor": cursor}),
        );
        seen.extend(
            out["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| m["id"].as_str().unwrap().parse::<u64>().unwrap()),
        );
        cursor = out["next_cursor"].as_str().map(str::to_string);
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(seen.len(), 450);
    assert!(
        seen.windows(2).all(|w| w[0] > w[1]),
        "newest first, once each"
    );
    let asked = t.source.asked_messages.lock().unwrap();
    assert!(asked.iter().all(|(_, _, limit)| *limit <= 100), "{asked:?}");
}

/// A page costs the same however deep it is, since its cursor holds where to read on.
#[test]
fn a_deep_page_reads_only_its_own_messages() {
    let t = DiscordTools::new(Canned {
        messages: Mutex::new(
            (0..2_000u64)
                .map(|i| message(3000000000000003000 - i))
                .collect(),
        ),
        ..Canned::new()
    });
    let mut cursor = None;
    for _ in 0..30 {
        let out = fits(
            &t,
            MESSAGES_TOOL,
            json!({"channel": GENERAL, "limit": 50, "cursor": cursor}),
        );
        assert_eq!(out["returned"], 50);
        cursor = out["next_cursor"].as_str().map(str::to_string);
    }
    assert_eq!(t.source.asked_messages.lock().unwrap().len(), 30);
    assert_eq!(*t.source.connects.lock().unwrap(), 30);
}

/// A moderator can delete what a page already returned.
#[test]
fn a_message_deleted_between_pages_skips_nothing() {
    let t = DiscordTools::new(Canned {
        messages: Mutex::new(
            (0..6u64)
                .map(|i| message(3000000000000000900 - i))
                .collect(),
        ),
        ..Canned::new()
    });
    let first = fits(&t, MESSAGES_TOOL, json!({"channel": GENERAL, "limit": 3}));
    let cursor = first["next_cursor"].as_str().unwrap().to_string();
    t.source
        .messages
        .lock()
        .unwrap()
        .retain(|m| m["id"] != "3000000000000000899");

    let second = fits(
        &t,
        MESSAGES_TOOL,
        json!({"channel": GENERAL, "limit": 3, "cursor": cursor}),
    );
    let ids: Vec<&str> = second["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "3000000000000000897",
            "3000000000000000896",
            "3000000000000000895"
        ]
    );
}

/// A cursor comes back from the client, and its key reaches a URL.
#[test]
fn a_cursor_holding_anything_but_an_id_is_refused_before_reading() {
    let filter = MessagesFilter {
        channel: GENERAL,
        guild: None,
        since: None,
        until: None,
    };
    let forged = ResumeRequest::new(&ListArgs::default(), &filter)
        .unwrap()
        .page(
            Vec::<(String, u32)>::new(),
            Some("1&after=2".to_string()),
            "since",
        )
        .unwrap()
        .info
        .next_cursor;
    let text = error_text(&call(
        &DiscordTools::new(Untouchable),
        MESSAGES_TOOL,
        json!({"channel": GENERAL, "cursor": forged}),
    ));
    assert!(text.starts_with("usage:"), "{text}");
}

/// The token is one keychain read, and one prompt, however many requests a call makes.
#[test]
fn a_call_connects_once_however_many_requests_it_makes() {
    let t = DiscordTools::new(Canned {
        messages: Mutex::new(
            (0..450u64)
                .map(|i| message(3000000000000000900 - i))
                .collect(),
        ),
        ..Canned::new()
    });
    let out = fits(
        &t,
        MESSAGES_TOOL,
        json!({"channel": "general", "guild": "another example", "limit": 200}),
    );
    assert_eq!(out["returned"], 200);
    assert!(t.source.asked_messages.lock().unwrap().len() > 1);
    assert_eq!(*t.source.connects.lock().unwrap(), 1);
}

#[test]
fn days_convert_to_the_snowflakes_discord_counts_from() {
    // The epoch itself, and a known date.
    assert_eq!(days_from_civil((1970, 1, 1)), 0);
    assert_eq!(days_from_civil((2015, 1, 1)) * MS_PER_DAY, DISCORD_EPOCH_MS);
    assert_eq!(snowflake_at_day("2015-01-01", 0).unwrap(), 0);
    assert_eq!(
        snowflake_at_day("2015-01-01", 1).unwrap(),
        (MS_PER_DAY as u64) << 22
    );
    // A leap year's end, across pk-cli-core's own conversion.
    let days = days_from_civil((2024, 12, 31));
    assert_eq!(
        pk_cli_core::dates::civil_from_unix(days * 86_400),
        (2024, 12, 31)
    );
    // Before Discord began is its first snowflake, not an overflow.
    assert_eq!(snowflake_at_day("2000-01-01", 0).unwrap(), 0);
}

fn id_on_day(date: &str, extra: u64) -> u64 {
    snowflake_at_day(date, 0).unwrap() + extra
}

#[test]
fn since_stops_the_walk_and_until_starts_it_further_back() {
    let newest = id_on_day("2020-01-03", 5);
    let middle = id_on_day("2020-01-02", 5);
    let oldest = id_on_day("2020-01-01", 5);
    let t = DiscordTools::new(Canned {
        messages: Mutex::new(vec![message(newest), message(middle), message(oldest)]),
        ..Canned::new()
    });
    let out = fits(
        &t,
        MESSAGES_TOOL,
        json!({"channel": GENERAL, "since": "2020-01-02"}),
    );
    let ids: Vec<&str> = out["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [newest.to_string(), middle.to_string()]);

    let out = fits(
        &t,
        MESSAGES_TOOL,
        json!({"channel": GENERAL, "until": "2020-01-02"}),
    );
    let ids: Vec<&str> = out["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [middle.to_string(), oldest.to_string()]);
    let asked = t.source.asked_messages.lock().unwrap().clone();
    assert_eq!(
        asked.last().unwrap().1.as_deref(),
        Some(id_on_day("2020-01-03", 0).to_string().as_str()),
        "the day after until, so all of until's day is read"
    );
}

#[test]
fn bad_dates_are_usage_errors_before_any_read() {
    let t = DiscordTools::new(Untouchable);
    for args in [
        json!({"channel": GENERAL, "since": "yesterday"}),
        json!({"channel": GENERAL, "until": "01/02/2000"}),
        json!({"channel": GENERAL, "since": "2000-01-02", "until": "2000-01-01"}),
        json!({"channel": "  "}),
        json!({"channel": GENERAL, "limit": 0}),
    ] {
        let text = error_text(&call(&t, MESSAGES_TOOL, args.clone()));
        assert!(text.starts_with("usage:"), "{args}: {text}");
    }
}

#[test]
fn a_token_discord_refuses_says_to_sign_in_and_a_403_does_not() {
    let t = DiscordTools::new(Canned {
        failure: Some(
            CliError::Auth("Discord rejected the bot token (HTTP 401)".into())
                .because(Reason::CredentialRefused),
        ),
        ..Canned::new()
    });
    let text = error_text(&call(&t, MESSAGES_TOOL, json!({"channel": GENERAL})));
    assert!(text.starts_with("auth:"), "{text}");
    assert!(text.contains("discord auth login"), "{text}");

    let t = DiscordTools::new(Canned {
        failure: Some(
            CliError::Auth("The bot lacks access (HTTP 403: Missing Access (code 50001))".into())
                .because(Reason::Forbidden),
        ),
        ..Canned::new()
    });
    let text = error_text(&call(&t, MESSAGES_TOOL, json!({"channel": GENERAL})));
    assert!(text.contains("Missing Access"), "{text}");
    assert!(text.contains("will not help"), "{text}");
    assert!(!text.contains("auth login"), "{text}");
}

#[test]
fn no_keychain_text_reaches_the_model() {
    let t = DiscordTools::new(Canned {
        failure: Some(CliError::Keychain(
            "reading piekstra.discord/bot-token: denied".into(),
        )),
        ..Canned::new()
    });
    let text = error_text(&call(&t, SERVERS_TOOL, json!({})));
    assert!(text.contains("keychain"), "{text}");
    assert!(!text.contains("bot-token"), "{text}");
    assert!(!text.contains("piekstra.discord"), "{text}");
}

#[test]
fn an_answer_that_is_not_a_list_is_upstream() {
    let t = DiscordTools::new(Canned {
        guilds: json!({"message": "not a list"}),
        ..Canned::new()
    });
    let text = error_text(&call(&t, SERVERS_TOOL, json!({})));
    assert!(text.starts_with("upstream:"), "{text}");
}

// ---- Live, over a local server ----------------------------------------------------

/// A plain HTTP server over the fixtures, keeping each request's first line and its
/// authorization header.
fn stub() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let log = seen.clone();
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut conn) = conn else { return };
            let mut buf = [0u8; 8192];
            let n = conn.read(&mut buf).unwrap_or(0);
            let head = String::from_utf8_lossy(&buf[..n]).to_string();
            let line = head.lines().next().unwrap_or("").to_string();
            let auth = head
                .lines()
                .find(|l| l.to_ascii_lowercase().starts_with("authorization:"))
                .unwrap_or("")
                .to_string();
            log.lock().unwrap().push(format!("{line} | {auth}"));
            let path = line.split_whitespace().nth(1).unwrap_or("");
            let body = if path.contains("/guilds/") {
                fixture("channels.json")
            } else if path.contains("/users/@me/guilds") {
                fixture("guilds.json")
            } else if path.contains("/messages") {
                fixture("messages.json")
            } else {
                "[]".to_string()
            };
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = conn.write_all(reply.as_bytes());
        }
    });
    (url, seen)
}

fn live_from(base: String, dir: &std::path::Path, source: SourceKind) -> DiscordTools<Live> {
    let store = ConfigStore::new("discord").with_override(Some(dir.join("config.json")));
    DiscordTools::new(Live {
        store,
        connect: Box::new(move |_| {
            Ok((Discord::at(&base, Secret::new("dummy-bot-token"))?, source))
        }),
    })
}

fn live(base: String, dir: &std::path::Path) -> DiscordTools<Live> {
    live_from(base, dir, SourceKind::Keychain)
}

/// A server that refuses every token.
fn refusing_stub() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut conn) = conn else { return };
            let mut buf = [0u8; 8192];
            let _ = conn.read(&mut buf);
            let body = r#"{"message": "401: Unauthorized", "code": 0}"#;
            let reply = format!(
                "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = conn.write_all(reply.as_bytes());
        }
    });
    url
}

/// Signing in again changes the keychain, and the variable outranks it.
#[test]
fn a_refused_token_sends_the_user_to_where_that_token_came_from() {
    let dir = tempfile::tempdir().unwrap();
    let t = live_from(refusing_stub(), dir.path(), SourceKind::Env);
    let text = error_text(&call(&t, SERVERS_TOOL, json!({})));
    assert!(text.starts_with("auth:"), "{text}");
    assert!(text.contains(TOKEN_ENV), "{text}");
    assert!(text.contains("settings"), "{text}");
    assert!(text.contains("will not help"), "{text}");

    let t = live_from(refusing_stub(), dir.path(), SourceKind::Keychain);
    let text = error_text(&call(&t, SERVERS_TOOL, json!({})));
    assert!(text.starts_with("auth:"), "{text}");
    assert!(text.contains("discord auth login"), "{text}");
    assert!(!text.contains(TOKEN_ENV), "{text}");
}

#[test]
fn every_request_any_tool_sends_is_a_get_with_the_bot_token() {
    let dir = tempfile::tempdir().unwrap();
    let (url, seen) = stub();
    let t = live(url, dir.path());
    fits(&t, SERVERS_TOOL, json!({}));
    fits(&t, CHANNELS_TOOL, json!({"guild": "Example Server"}));
    fits(
        &t,
        MESSAGES_TOOL,
        json!({"channel": "#general", "guild": GUILD, "limit": 2}),
    );
    let seen = seen.lock().unwrap().clone();
    assert!(seen.len() >= 5, "{seen:?}");
    for request in &seen {
        assert!(request.starts_with("GET /"), "{request}");
        assert!(request.ends_with("Bot dummy-bot-token"), "{request}");
    }
    assert!(
        seen.iter()
            .any(|r| r.contains(&format!("/channels/{GENERAL}/messages?limit=3"))),
        "{seen:?}"
    );
}

#[test]
fn a_broken_config_is_one_calls_error_and_names_no_path() {
    let dir = tempfile::tempdir().unwrap();
    let (url, _) = stub();
    let t = live(url, dir.path());
    std::fs::write(dir.path().join("config.json"), "{not json").unwrap();
    assert_eq!(tools_list_json(&t)["tools"].as_array().unwrap().len(), 3);
    let text = error_text(&call(&t, CHANNELS_TOOL, json!({})));
    assert!(text.contains("discord config show"), "{text}");
    assert!(!text.contains(dir.path().to_str().unwrap()), "{text}");
}

#[test]
fn the_fixtures_hold_only_dummies() {
    for guild in fixture_json("guilds.json").as_array().unwrap() {
        assert!(guild["id"]
            .as_str()
            .unwrap()
            .starts_with("100000000000000000"));
        assert!(guild["name"].as_str().unwrap().contains("Example"));
    }
    for message in fixture_json("messages.json").as_array().unwrap() {
        assert!(message["timestamp"]
            .as_str()
            .unwrap()
            .starts_with("2000-01-"));
        assert!(message["author"]["username"]
            .as_str()
            .unwrap()
            .ends_with("-user"));
    }
}
