//! Runs the `tgt` binary for commands that don't need Telegram.

use std::{path::Path, process::Command};

fn tgt(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tgt"));
    command
        .env_clear()
        .env("HOME", home)
        .env("PATH", std::env::var_os("PATH").unwrap_or_default());
    command
}

#[test]
fn help_lists_subcommands() {
    let home = tempfile::tempdir().unwrap();
    let output = tgt(home.path()).arg("--help").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    for subcommand in ["logout", "send", "init-config", "clear"] {
        assert!(stdout.contains(subcommand), "--help mentions {subcommand}");
    }
}

#[test]
fn version_matches_the_package() {
    let home = tempfile::tempdir().unwrap();
    let output = tgt(home.path()).arg("--version").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.trim(), format!("tgt {}", env!("CARGO_PKG_VERSION")));
}

#[test]
fn init_config_then_clear() {
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join(".config/tgt");

    let output = tgt(home.path()).arg("init-config").output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(config.join("config.toml").exists());
    assert!(config.join("keymap.toml").exists());
    assert!(config.join("themes/theme.toml").exists());

    let output = tgt(home.path())
        .args(["clear", "--config", "--yes"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(!config.exists());
}

#[test]
fn invalid_config_is_reported() {
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join(".config/tgt");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(config.join("config.toml"), "[app\n").unwrap();

    let output = tgt(home.path()).arg("init-config").output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.starts_with("tgt: "), "{stderr}");
}
