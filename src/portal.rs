//! Runner for the interactive developer-portal helper (`portal/portal.mjs`).
//!
//! The helper is embedded in the binary and staged into
//! `${XDG_CACHE_HOME:-~/.cache}/discord-cli/portal/`, where npm installs
//! Playwright on first use and a persistent Chromium profile keeps the
//! user's portal login between runs (delete it with `discord app logout`).
//! The helper writes one JSON document to stdout; its stderr (progress,
//! login prompts) passes straight through to the user.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use pk_cli_core::CliError;
use serde_json::Value;

const PORTAL_MJS: &str = include_str!("../portal/portal.mjs");
const PACKAGE_JSON: &str = include_str!("../portal/package.json");

fn portal_dir() -> Result<PathBuf, CliError> {
    let base = match std::env::var("XDG_CACHE_HOME") {
        Ok(x) if !x.is_empty() => PathBuf::from(x),
        _ => {
            let home = std::env::var("HOME").map_err(|_| {
                CliError::Other("cannot locate home directory ($HOME unset)".into())
            })?;
            PathBuf::from(home).join(".cache")
        }
    };
    Ok(base.join("discord-cli").join("portal"))
}

/// The persistent browser profile (the user's portal login lives here).
pub fn profile_dir() -> Result<PathBuf, CliError> {
    Ok(portal_dir()?.join("profile"))
}

/// Run one portal operation and return its JSON result. `op_item` is an
/// optional 1Password item **name** (not a secret) the helper uses to
/// auto-login — but only lazily, if it actually hits the login form, so an
/// already-valid browser session never triggers a keychain/`op` prompt.
pub fn run_op(
    op: &str,
    args: &[&str],
    op_item: Option<&str>,
    quiet: bool,
) -> Result<Value, CliError> {
    let dir = ensure_helper(quiet)?;
    let mut cmd = Command::new("node");
    cmd.arg(dir.join("portal.mjs"))
        .arg(op)
        .args(args)
        .env("DISCORD_CLI_PORTAL_PROFILE", profile_dir()?)
        .current_dir(&dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    if let Some(item) = op_item {
        cmd.env("DISCORD_CLI_PORTAL_OP_ITEM", item);
    }
    let out = cmd.output().map_err(node_missing)?;

    if !out.status.success() {
        return Err(match out.status.code() {
            Some(2) => CliError::Usage("portal helper rejected the arguments".into()),
            Some(3) => {
                CliError::Auth("no Discord login — log in in the browser window and retry".into())
            }
            _ => CliError::Upstream("portal helper failed (see messages above)".into()),
        });
    }
    parse_stdout(&String::from_utf8_lossy(&out.stdout))
}

/// Stage the helper files and install Playwright on first use.
fn ensure_helper(quiet: bool) -> Result<PathBuf, CliError> {
    let dir = portal_dir()?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| CliError::Other(format!("creating {}: {e}", dir.display())))?;
    // Always rewrite so the helper matches this binary's version.
    for (name, content) in [("portal.mjs", PORTAL_MJS), ("package.json", PACKAGE_JSON)] {
        std::fs::write(dir.join(name), content)
            .map_err(|e| CliError::Other(format!("writing {name}: {e}")))?;
    }
    if !dir.join("node_modules").exists() {
        if !quiet {
            eprintln!("setting up the portal helper (one-time npm install of Playwright)…");
        }
        run_setup(
            Command::new("npm")
                .args(["install", "--no-fund", "--no-audit", "--loglevel=error"])
                .current_dir(&dir),
            "npm install",
        )?;
        // No-op when a matching Chromium build is already cached.
        run_setup(
            Command::new(dir.join("node_modules/.bin/playwright"))
                .args(["install", "chromium"])
                .current_dir(&dir),
            "playwright install chromium",
        )?;
    }
    Ok(dir)
}

fn run_setup(cmd: &mut Command, what: &str) -> Result<(), CliError> {
    let status = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()
        .map_err(node_missing)?;
    if !status.success() {
        return Err(CliError::Other(format!("{what} failed")));
    }
    Ok(())
}

fn node_missing(e: std::io::Error) -> CliError {
    if e.kind() == std::io::ErrorKind::NotFound {
        CliError::Other(
            "Node.js not found — portal commands drive a real browser via Playwright \
             and need `node`/`npm` on $PATH (https://nodejs.org)"
                .into(),
        )
    } else {
        CliError::Other(format!("running the portal helper: {e}"))
    }
}

/// The helper prints exactly one JSON document as its final stdout line.
fn parse_stdout(stdout: &str) -> Result<Value, CliError> {
    let line = stdout
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .ok_or_else(|| CliError::Upstream("portal helper produced no output".into()))?;
    serde_json::from_str(line)
        .map_err(|e| CliError::Upstream(format!("parsing portal helper output: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stdout_parsing_takes_the_last_json_line() {
        let v = parse_stdout("noise\n{\"token\": \"abc\"}\n").unwrap();
        assert_eq!(v["token"], "abc");
        assert!(parse_stdout("").is_err());
        assert!(parse_stdout("not json\n").is_err());
    }
}
