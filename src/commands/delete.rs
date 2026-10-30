//! `discord delete` — remove a message from a channel.
//!
//! Deletion is destructive, so it confirms by default: interactively it
//! prompts, non-interactively it refuses without `--force` (exit 6), matching
//! the family's mutation-safety convention.

use std::io::{IsTerminal, Write};

use pk_cli_core::{output, CliError};
use serde_json::json;

use crate::cli::DeleteArgs;
use crate::commands::Ctx;

pub fn run(ctx: &Ctx, args: &DeleteArgs) -> Result<(), CliError> {
    let dc = ctx.connect()?;
    let channel_id = ctx.resolve_channel(&dc, &args.channel, args.guild.as_deref())?;

    if !args.force && !confirm(ctx, &channel_id, &args.message)? {
        if !ctx.cli.common.quiet {
            eprintln!("cancelled");
        }
        return Ok(());
    }

    dc.delete_message(&channel_id, &args.message)?;

    let dto = json!({
        "schema": "deleted/v1",
        "channel_id": channel_id,
        "message_id": args.message,
        "deleted": true,
    });
    if ctx.cli.common.json {
        output::json(&dto);
    } else if !ctx.cli.common.quiet {
        eprintln!("deleted message {} from channel {channel_id}", args.message);
    }
    Ok(())
}

/// Prompt on a TTY; refuse non-interactively without `--force` (exit 6).
fn confirm(ctx: &Ctx, channel_id: &str, message_id: &str) -> Result<bool, CliError> {
    if ctx.cli.common.json || !std::io::stdin().is_terminal() {
        return Err(CliError::ConfirmationRequired(format!(
            "refusing to delete message {message_id} without --force (non-interactive)"
        )));
    }
    eprint!("delete message {message_id} from channel {channel_id}? [y/N] ");
    std::io::stderr().flush().ok();
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| CliError::Other(format!("reading confirmation: {e}")))?;
    Ok(matches!(input.trim().to_lowercase().as_str(), "y" | "yes"))
}
