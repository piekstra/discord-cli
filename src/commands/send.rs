//! `discord send` — post a message to a channel (`message/v1`).
//!
//! The text comes from the positional argument or, when omitted, from stdin
//! (so `some-command | discord send <CHANNEL>` works). Discord caps message
//! content at 2000 characters; we reject longer input client-side.

use std::io::Read;

use pk_cli_core::{output, CliError};
use serde_json::json;

use crate::cli::SendArgs;
use crate::commands::Ctx;

const MAX_CONTENT_CHARS: usize = 2000;

pub fn run(ctx: &Ctx, args: &SendArgs) -> Result<(), CliError> {
    let content = match &args.message {
        Some(m) => m.clone(),
        None => read_stdin_content()?,
    };
    validate_content(&content)?;

    let dc = ctx.connect()?;
    let channel_id = ctx.resolve_channel(&dc, &args.channel, args.guild.as_deref())?;
    let sent = dc.create_message(&channel_id, &content, args.reply_to.as_deref())?;

    let dto = json!({
        "schema": "message/v1",
        "id": sent.get("id"),
        "channel_id": channel_id,
        "timestamp": sent.get("timestamp"),
        "content": sent.get("content"),
    });
    if ctx.cli.common.json {
        output::json(&dto);
    } else if !ctx.cli.common.quiet {
        eprintln!(
            "sent message {} to channel {channel_id}",
            dto["id"].as_str().unwrap_or("?")
        );
    }
    Ok(())
}

fn read_stdin_content() -> Result<String, CliError> {
    use std::io::IsTerminal;
    if std::io::stdin().is_terminal() {
        return Err(CliError::Usage(
            "no message given — pass MESSAGE or pipe the text on stdin".into(),
        ));
    }
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .map_err(|e| CliError::Other(format!("reading message from stdin: {e}")))?;
    Ok(buf.trim_end_matches('\n').to_string())
}

fn validate_content(content: &str) -> Result<(), CliError> {
    if content.trim().is_empty() {
        return Err(CliError::Usage("refusing to send an empty message".into()));
    }
    let chars = content.chars().count();
    if chars > MAX_CONTENT_CHARS {
        return Err(CliError::Usage(format!(
            "message is {chars} characters; Discord's limit is {MAX_CONTENT_CHARS}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_bounds_are_enforced() {
        assert!(validate_content("hello").is_ok());
        assert!(validate_content("  \n ").is_err());
        assert!(validate_content(&"x".repeat(2000)).is_ok());
        assert!(validate_content(&"x".repeat(2001)).is_err());
    }
}
