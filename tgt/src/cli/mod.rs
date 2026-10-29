//! Command-line definitions. Parsed only in `main`, then converted into
//! [`CliOverrides`] so no `clap` type crosses the settings boundary.

use std::path::PathBuf;

use clap::{ArgAction, Parser, Subcommand};

use crate::{
    config::LogLevel,
    settings::{CliOverrides, Command as SettingsCommand},
};

#[cfg(test)]
mod tests;

/// A simple TUI for Telegram.
#[derive(Debug, Parser)]
#[command(name = "tgt", version, about)]
pub struct Cli {
    /// Directory with config.toml, keymap.toml and themes/ [env: TGT_CONFIG_DIR] [default: $XDG_CONFIG_HOME/tgt].
    #[arg(long, global = true, value_name = "DIR")]
    pub config_dir: Option<PathBuf>,

    /// Directory for logs [default: $XDG_STATE_HOME/tgt/logs].
    #[arg(long, global = true, value_name = "DIR")]
    pub log_dir: Option<PathBuf>,

    /// Log level for tgt's own log.
    #[arg(long, global = true, value_enum, conflicts_with_all = ["verbose", "quiet"])]
    pub log_level: Option<LogLevel>,

    /// Log more: -v debug, -vv trace.
    #[arg(short, long, global = true, action = ArgAction::Count, conflicts_with = "quiet")]
    pub verbose: u8,

    /// Log errors only.
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Theme to use, by name from themes/ (overrides `app.theme`).
    #[arg(long, value_name = "NAME")]
    pub theme: Option<String>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Log out of Telegram.
    Logout,
    /// Send a message to a chat and exit.
    Send {
        /// Chat title to search for; must match exactly one chat.
        chat: String,
        /// Message text.
        message: String,
    },
    /// Write the default config files to the config directory.
    InitConfig {
        /// Overwrite existing files.
        #[arg(long)]
        force: bool,
    },
    /// Remove tgt's config, data and/or state directories.
    Clear {
        /// Remove the config directory.
        #[arg(short, long)]
        config: bool,
        /// Remove the data directory (TDLib database and downloads).
        #[arg(short, long)]
        data: bool,
        /// Remove the state directory (logs and remembered theme).
        #[arg(short, long)]
        state: bool,
        /// Remove all of the above.
        #[arg(long)]
        all: bool,
        /// Don't ask for confirmation.
        #[arg(short, long)]
        yes: bool,
    },
}

impl Cli {
    fn log_level(&self) -> Option<LogLevel> {
        match (self.log_level, self.verbose, self.quiet) {
            (Some(level), ..) => Some(level),
            (None, _, true) => Some(LogLevel::Error),
            (None, 0, false) => None,
            (None, 1, false) => Some(LogLevel::Debug),
            (None, _, false) => Some(LogLevel::Trace),
        }
    }
}

impl From<Cli> for CliOverrides {
    fn from(cli: Cli) -> Self {
        let log_level = cli.log_level();
        Self {
            config_dir: cli.config_dir,
            log_dir: cli.log_dir,
            log_level,
            theme: cli.theme,
            command: cli
                .command
                .map_or(SettingsCommand::Tui, SettingsCommand::from),
        }
    }
}

impl From<Command> for SettingsCommand {
    fn from(command: Command) -> Self {
        match command {
            Command::Logout => Self::Logout,
            Command::Send { chat, message } => Self::Send { chat, message },
            Command::InitConfig { force } => Self::InitConfig { force },
            Command::Clear {
                config,
                data,
                state,
                all,
                yes,
            } => Self::Clear {
                config: config || all,
                data: data || all,
                state: state || all,
                yes,
            },
        }
    }
}
