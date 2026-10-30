//! `discord auth` — bot-token storage, verification, and status.
//!
//! The credential is a Discord **bot token** (developer portal → your app →
//! Bot → Reset Token). It enters via `--stdin`, `--from-env <VAR>`, or an
//! interactive no-echo prompt — never argv — and lives only in the OS
//! keychain under the family service `piekstra.discord`.

use pk_cli_auth::{AuthMethod, AuthStatus, LoginArgs, LogoutArgs, SetCredentialArgs};
use pk_cli_core::CliError;
use pk_cli_secrets::Secret;
use serde_json::Value;

use crate::cli::AuthCmd;
use crate::client::Discord;
use crate::commands::{Ctx, ACCOUNT};
use crate::config::Config;

pub fn run(ctx: &Ctx, cmd: &AuthCmd) -> Result<(), CliError> {
    match cmd {
        AuthCmd::Login(args) => login(ctx, args),
        AuthCmd::Status => status(ctx),
        AuthCmd::Logout(args) => logout(ctx, args),
        AuthCmd::SetCredential(args) => set_credential(ctx, args),
    }
}

fn login(ctx: &Ctx, args: &LoginArgs) -> Result<(), CliError> {
    if ctx.creds.get(ACCOUNT)?.is_some() && !args.overwrite {
        return Err(CliError::Usage(
            "a bot token is already stored; pass --overwrite to replace it".into(),
        ));
    }
    let prompt = if args.non_interactive {
        None
    } else {
        Some("Bot token")
    };
    let token = normalize_token(&args.source.read(prompt)?)?;

    let mut verified_as = None;
    if !args.no_verify {
        let dc = Discord::new(Secret::new(token.expose()))?;
        let me = dc.me()?;
        verified_as = me
            .get("username")
            .and_then(Value::as_str)
            .map(str::to_string);
    }

    ctx.creds.set(ACCOUNT, &token)?;
    let mut cfg: Config = ctx.config()?;
    cfg.username = verified_as.clone();
    ctx.store.save(&cfg)?;

    if !ctx.cli.common.quiet {
        match verified_as {
            Some(name) => eprintln!("bot token stored in the OS keychain (verified as {name})"),
            None => eprintln!("bot token stored in the OS keychain (not verified)"),
        }
    }
    Ok(())
}

fn status(ctx: &Ctx) -> Result<(), CliError> {
    let stored = ctx.creds.get(ACCOUNT)?.is_some();
    let mut status = AuthStatus::new(true, stored, AuthMethod::Password);
    status.username = ctx.config()?.username;
    status.credential_in_keychain = Some(stored);
    status.emit(ctx.cli.common.json);
    Ok(())
}

fn logout(ctx: &Ctx, args: &LogoutArgs) -> Result<(), CliError> {
    if args.forget {
        let removed = ctx.creds.delete(ACCOUNT)?;
        ctx.store.clear()?;
        if !ctx.cli.common.quiet {
            eprintln!(
                "{}",
                if removed {
                    "bot token removed from the keychain"
                } else {
                    "no bot token was stored"
                }
            );
        }
    } else if !ctx.cli.common.quiet {
        // Bot tokens have no server-side session to end.
        eprintln!("nothing to do — pass --forget to remove the stored bot token");
    }
    Ok(())
}

fn set_credential(ctx: &Ctx, args: &SetCredentialArgs) -> Result<(), CliError> {
    if ctx.creds.get(ACCOUNT)?.is_some() && !args.overwrite {
        return Err(CliError::Usage(
            "a bot token is already stored; pass --overwrite to replace it".into(),
        ));
    }
    let token = normalize_token(&args.source.read(None)?)?;
    ctx.creds.set(ACCOUNT, &token)?;
    if !ctx.cli.common.quiet {
        eprintln!("bot token stored");
    }
    Ok(())
}

/// Trim whitespace and a pasted `Bot ` prefix; reject an empty result.
fn normalize_token(raw: &Secret) -> Result<Secret, CliError> {
    let trimmed = raw.expose().trim();
    let trimmed = trimmed.strip_prefix("Bot ").unwrap_or(trimmed).trim();
    if trimmed.is_empty() {
        return Err(CliError::Usage("empty bot token — nothing stored".into()));
    }
    Ok(Secret::new(trimmed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_prefix_and_whitespace() {
        let t = normalize_token(&Secret::new("  Bot abc.def.ghi \n")).unwrap();
        assert_eq!(t.expose(), "abc.def.ghi");
        let t = normalize_token(&Secret::new("abc.def.ghi")).unwrap();
        assert_eq!(t.expose(), "abc.def.ghi");
        assert!(normalize_token(&Secret::new("   \n")).is_err());
        assert!(normalize_token(&Secret::new("")).is_err());
    }
}
