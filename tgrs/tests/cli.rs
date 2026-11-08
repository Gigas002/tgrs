//! Runs the `tgrs` binary for commands that don't need Telegram.

use std::{path::Path, process::Command};

fn tgrs(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tgrs"));
    command
        .env_clear()
        .env("HOME", home)
        .env("PATH", std::env::var_os("PATH").unwrap_or_default());
    // A TDLib installed outside the system paths (as in CI) is found through these
    for var in ["LD_LIBRARY_PATH", "DYLD_LIBRARY_PATH"] {
        if let Some(value) = std::env::var_os(var) {
            command.env(var, value);
        }
    }
    command
}

#[test]
fn help_lists_only_logout() {
    let home = tempfile::tempdir().unwrap();
    let output = tgrs(home.path()).arg("--help").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("logout"), "--help mentions logout");
    for removed in ["send", "init-config", "clear"] {
        assert!(!stdout.contains(removed), "--help doesn't offer {removed}");
    }
}

#[test]
fn messages_cant_be_sent_from_the_command_line() {
    let home = tempfile::tempdir().unwrap();
    let output = tgrs(home.path())
        .args(["send", "Saved Messages", "hi"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn version_matches_the_package() {
    let home = tempfile::tempdir().unwrap();
    let output = tgrs(home.path()).arg("--version").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.trim(), format!("tgrs {}", env!("CARGO_PKG_VERSION")));
}

#[test]
fn files_are_managed_by_hand() {
    // Defaults apply when files are missing; users edit and remove files themselves
    let home = tempfile::tempdir().unwrap();
    for args in [&["init-config"][..], &["clear", "--all", "--yes"]] {
        let output = tgrs(home.path()).args(args).output().unwrap();
        assert!(!output.status.success(), "{args:?} is not a command");
    }
    assert!(!home.path().join(".config/tgrs").exists());
}

#[test]
fn invalid_config_is_reported() {
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join(".config/tgrs");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(config.join("config.toml"), "[app\n").unwrap();

    let output = tgrs(home.path()).arg("logout").output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.starts_with("tgrs: "), "{stderr}");
}

#[test]
fn login_stops_when_input_ends() {
    // TDLib's start-up isn't logged to the terminal, and closed input quits
    // instead of asking again forever
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join(".config/tgrs");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(
        config.join("config.toml"),
        "[credentials]\nstore = \"plain\"\n",
    )
    .unwrap();

    let output = tgrs(home.path())
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        stdout.matches("Enter your phone number").count(),
        1,
        "{stdout}"
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(!stderr.contains("Client.cpp"), "{stderr}");
}
