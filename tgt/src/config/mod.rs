//! Discovery, reading and deserialization of tgt's TOML files:
//! `config.toml`, `keymap.toml` and `themes/*.toml`.
//!
//! Every field is optional; precedence and defaults are applied in `settings`.

use std::{
    io,
    path::{Path, PathBuf},
};

use serde::Deserialize;

pub mod keymap;
pub mod theme;

#[cfg(test)]
mod tests;

pub use keymap::KeymapFile;
pub use theme::{ThemeEntry, ThemeFile};

pub const FILE_NAME: &str = "config.toml";

/// Default files written by `tgt init-config`, relative to the config directory.
pub const BUNDLED_FILES: &[(&str, &str)] = &[
    (FILE_NAME, include_str!("../../../examples/config.toml")),
    (keymap::FILE_NAME, keymap::BUNDLED),
    (
        "themes/catppuccin.toml",
        include_str!("../../../examples/themes/catppuccin.toml"),
    ),
    (
        "themes/first_theme.toml",
        include_str!("../../../examples/themes/first_theme.toml"),
    ),
    (
        "themes/github.toml",
        include_str!("../../../examples/themes/github.toml"),
    ),
    (
        "themes/gruvbox.toml",
        include_str!("../../../examples/themes/gruvbox.toml"),
    ),
    (
        "themes/monokai.toml",
        include_str!("../../../examples/themes/monokai.toml"),
    ),
    (
        "themes/nord.toml",
        include_str!("../../../examples/themes/nord.toml"),
    ),
    (
        "themes/onedark.toml",
        include_str!("../../../examples/themes/onedark.toml"),
    ),
    (
        "themes/theme.toml",
        include_str!("../../../examples/themes/theme.toml"),
    ),
    (
        "themes/tokyo.toml",
        include_str!("../../../examples/themes/tokyo.toml"),
    ),
];

/// Raw contents of `config.toml`. Unknown keys are ignored.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct FileConfig {
    pub app: AppFile,
    pub logger: LoggerFile,
    pub telegram: TelegramFile,
}

/// `[app]`: terminal and UI behavior.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct AppFile {
    pub mouse_support: Option<bool>,
    pub paste_support: Option<bool>,
    pub frame_rate: Option<f64>,
    pub show_status_bar: Option<bool>,
    pub show_title_bar: Option<bool>,
    pub theme_enable: Option<bool>,
    /// Theme name, i.e. a file stem in `themes/`.
    pub theme: Option<String>,
    pub use_emoji_icons: Option<bool>,
    pub photo_max_dimension: Option<u32>,
    pub photo_viewer_popup_size: Option<f32>,
}

/// `[logger]`: tgt's own log.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct LoggerFile {
    pub level: Option<LogLevel>,
    pub dir: Option<PathBuf>,
    pub rotation: Option<Rotation>,
    pub keep: Option<usize>,
}

/// `[telegram]`: TDLib parameters.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct TelegramFile {
    pub api_id: Option<i32>,
    pub api_hash: Option<String>,
    pub database_dir: Option<PathBuf>,
    pub use_file_database: Option<bool>,
    pub use_chat_info_database: Option<bool>,
    pub use_message_database: Option<bool>,
    pub system_language_code: Option<String>,
    pub device_model: Option<String>,
    pub log_verbosity: Option<i32>,
    pub log_path: Option<PathBuf>,
    pub log_redirect_stderr: Option<bool>,
    pub proxy: Option<ProxyFile>,
}

/// `[telegram.proxy]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct ProxyFile {
    pub r#type: Option<ProxyType>,
    pub server: Option<String>,
    pub port: Option<i32>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub http_only: Option<bool>,
    pub secret: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyType {
    #[default]
    Socks5,
    Http,
    Mtproto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Off,
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rotation {
    Minutely,
    Hourly,
    #[default]
    Daily,
    Never,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read {}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },

    #[error("invalid {}: {source}", path.display())]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
}

/// Loads `config.toml` from `config_dir`; a missing file is `Ok(None)`.
pub fn load(config_dir: &Path) -> Result<Option<FileConfig>, ConfigError> {
    read_toml(&config_dir.join(FILE_NAME))
}

/// Parses `config.toml` contents.
pub fn parse(content: &str) -> Result<FileConfig, toml::de::Error> {
    toml::from_str(content)
}

/// Reads and deserializes a TOML file; a missing file is `Ok(None)`.
pub(crate) fn read_toml<T: serde::de::DeserializeOwned>(
    path: &Path,
) -> Result<Option<T>, ConfigError> {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(ConfigError::Io {
                path: path.to_path_buf(),
                source,
            });
        }
    };

    toml::from_str(&content)
        .map(Some)
        .map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })
}
