use clap::{CommandFactory, Parser};

use super::*;

fn overrides(args: &[&str]) -> CliOverrides {
    Cli::try_parse_from(std::iter::once("tgrs").chain(args.iter().copied()))
        .unwrap()
        .into()
}

#[test]
fn cli_definition_is_valid() {
    Cli::command().debug_assert();
}

#[test]
fn no_arguments_opens_the_tui() {
    assert_eq!(overrides(&[]), CliOverrides::default());
}

#[test]
fn verbosity_flags_map_to_levels() {
    assert_eq!(overrides(&["-v"]).log_level, Some(LogLevel::Debug));
    assert_eq!(overrides(&["-vv"]).log_level, Some(LogLevel::Trace));
    assert_eq!(overrides(&["-q"]).log_level, Some(LogLevel::Error));
    assert_eq!(
        overrides(&["--log-level", "warn"]).log_level,
        Some(LogLevel::Warn)
    );
}

#[test]
fn log_level_conflicts_with_verbose() {
    assert!(Cli::try_parse_from(["tgrs", "--log-level", "warn", "-v"]).is_err());
}

#[test]
fn there_is_no_send_command() {
    // Telegram is only used through the TUI; scripts can't send messages
    assert!(Cli::try_parse_from(["tgrs", "send", "Saved Messages", "hi"]).is_err());
}

#[test]
fn global_flags_work_after_subcommand() {
    let cli = overrides(&["logout", "--config-dir", "/tmp/tgrs"]);

    assert_eq!(
        cli.config_dir.as_deref(),
        Some(std::path::Path::new("/tmp/tgrs"))
    );
    assert_eq!(cli.command, SettingsCommand::Logout);
}
