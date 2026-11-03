//! Black-box checks of the `discord` binary's offline surface. Each run gets
//! its own config and cache dirs, so no test reads the real config.

use std::process::Command;

fn discord(home: &std::path::Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_discord"));
    cmd.env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env_remove("NO_COLOR");
    cmd
}

/// A fresh dir per call; the counter keeps parallel tests apart.
fn tmp() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "discord-cli-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn no_color_in_the_environment_never_fails_a_command() {
    // Any non-empty NO_COLOR disables color (no-color.org); parsed as a
    // bool, the conventional NO_COLOR=1 was a usage error.
    let home = tmp();
    for v in ["1", "true", "0", ""] {
        let out = discord(&home)
            .env("NO_COLOR", v)
            .args(["completions", "bash"])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "NO_COLOR={v:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
