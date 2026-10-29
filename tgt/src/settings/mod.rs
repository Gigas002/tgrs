//! Unified resolver: CLI > environment > config files > built-in defaults.
//!
//! [`Settings`] is the only configuration type passed below `main`.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use libtgt::client::{Proxy, ProxyKind, TdLogging, TdParams};

use crate::{
    config::{
        self, ConfigError, FileConfig, LogLevel, ProxyFile, ProxyType, Rotation,
        theme::{self as theme_file, ThemeSource},
    },
    utils::{self, AppDirs, PathsError, XdgDirs},
};

pub mod keymap;
pub mod theme;

#[cfg(test)]
mod tests;

pub use keymap::{ActionBinding, KeyMap, Keymap, KeymapError};
pub use theme::{DEFAULT_THEME, Theme, ThemeError};

/// Overrides the config directory, like `--config-dir`.
pub const CONFIG_DIR_ENV: &str = "TGT_CONFIG_DIR";
/// Override `telegram.api_id` / `telegram.api_hash`.
pub const API_ID_ENV: &str = "API_ID";
pub const API_HASH_ENV: &str = "API_HASH";

/// tgt's public Telegram API credentials, from <https://my.telegram.org>.
pub const DEFAULT_API_ID: i32 = 94575;
pub const DEFAULT_API_HASH: &str = "a3406de8d171bb422bb6ddf3bbd800e2";
pub const DEFAULT_FRAME_RATE: f64 = 60.0;
pub const DEFAULT_PHOTO_MAX_DIMENSION: u32 = 1920;
pub const DEFAULT_PHOTO_VIEWER_POPUP_SIZE: f32 = 0.8;
pub const DEFAULT_LOG_KEEP: usize = 7;
pub const DEFAULT_TDLIB_VERBOSITY: i32 = 2;
pub const DEFAULT_PROXY_PORT: i32 = 1080;

/// File in the state directory that remembers the theme picked in the UI.
pub const THEME_STATE_FILE: &str = "theme";

/// Values taken from the command line, already converted from `clap` types.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CliOverrides {
    pub config_dir: Option<PathBuf>,
    pub log_dir: Option<PathBuf>,
    pub log_level: Option<LogLevel>,
    pub theme: Option<String>,
    pub command: Command,
}

/// What this invocation should do.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Command {
    /// Open the TUI.
    #[default]
    Tui,
    /// Log out of Telegram.
    Logout,
    /// Send one message and exit.
    Send { chat: String, message: String },
    /// Write the default config files.
    InitConfig { force: bool },
    /// Remove tgt's directories.
    Clear {
        config: bool,
        data: bool,
        state: bool,
        yes: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub dirs: Dirs,
    pub app: App,
    pub logging: Logging,
    pub telegram: TdParams,
    pub tdlib_log: TdLogging,
    pub keymap: Keymap,
    pub theme: Theme,
    /// Every theme the UI can switch to.
    pub themes: Vec<(String, ThemeSource)>,
    pub command: Command,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dirs {
    pub home: PathBuf,
    /// Holds `config.toml`, `keymap.toml` and `themes/`.
    pub config: PathBuf,
    pub data: PathBuf,
    pub state: PathBuf,
}

impl Dirs {
    /// Where the theme picked in the UI is remembered.
    pub fn theme_state_file(&self) -> PathBuf {
        self.state.join(THEME_STATE_FILE)
    }
}

/// `[app]`, resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct App {
    pub mouse_support: bool,
    pub paste_support: bool,
    pub frame_rate: f64,
    pub show_status_bar: bool,
    pub show_title_bar: bool,
    pub theme_enable: bool,
    pub use_emoji_icons: bool,
    /// Longer side of displayed photos in pixels; 0 disables downscaling.
    pub photo_max_dimension: u32,
    /// Photo viewer size as a fraction of the terminal (0.01–1.0).
    pub photo_viewer_popup_size: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Logging {
    pub level: LogLevel,
    pub dir: PathBuf,
    pub rotation: Rotation,
    /// Rotated log files to keep.
    pub keep: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error(transparent)]
    Paths(#[from] PathsError),

    #[error(transparent)]
    Config(#[from] ConfigError),

    #[error(transparent)]
    Keymap(#[from] KeymapError),

    #[error(transparent)]
    Theme(#[from] ThemeError),

    #[error("failed to resolve {}: {source}", path.display())]
    Absolute {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("config.toml: `{key}` must be an absolute path (or start with `~`), got {}", path.display())]
    RelativePath { key: &'static str, path: PathBuf },

    #[error("{API_ID_ENV} must be an integer, got {0:?}")]
    ApiId(String),

    #[error("config.toml: `logger.keep` must be at least 1")]
    LogKeep,

    #[error("config.toml: `app.frame_rate` must be positive")]
    FrameRate,
}

impl Settings {
    /// Resolves settings from CLI values, config files and the environment
    /// (`lookup` is typically `std::env::var_os`).
    pub fn resolve<F>(cli: CliOverrides, lookup: F) -> Result<Self, SettingsError>
    where
        F: Fn(&str) -> Option<OsString>,
    {
        let xdg = XdgDirs::from_lookup(&lookup)?;
        let defaults = AppDirs::new(&xdg);

        let config_dir = match cli
            .config_dir
            .or_else(|| lookup(CONFIG_DIR_ENV).map(PathBuf::from))
        {
            Some(dir) => absolute(dir)?,
            None => defaults.config,
        };
        let file = config::load(&config_dir)?.unwrap_or_default();
        let from_config = |key, path| config_path(key, path, &xdg.home);

        let dirs = Dirs {
            home: xdg.home.clone(),
            config: config_dir,
            data: defaults.data,
            state: defaults.state,
        };

        let logging = Logging {
            level: cli.log_level.or(file.logger.level).unwrap_or_default(),
            dir: match cli.log_dir {
                Some(dir) => absolute(dir)?,
                None => from_config("logger.dir", file.logger.dir.clone())?
                    .unwrap_or_else(|| dirs.state.join("logs")),
            },
            rotation: file.logger.rotation.unwrap_or_default(),
            keep: match file.logger.keep.unwrap_or(DEFAULT_LOG_KEEP) {
                0 => return Err(SettingsError::LogKeep),
                keep => keep,
            },
        };

        let app = resolve_app(&file)?;
        let telegram = resolve_telegram(&file, &dirs, &cli.command, &lookup, &from_config)?;
        let tdlib_log = TdLogging {
            verbosity_level: file
                .telegram
                .log_verbosity
                .unwrap_or(DEFAULT_TDLIB_VERBOSITY),
            path: from_config("telegram.log_path", file.telegram.log_path.clone())?
                .unwrap_or_else(|| dirs.state.join("tdlib.log")),
            redirect_stderr: file.telegram.log_redirect_stderr.unwrap_or(false),
        };

        let keymap = Keymap::resolve(
            &config::keymap::bundled(),
            config::keymap::load(&dirs.config)?.as_ref(),
        )?;

        let themes = theme_file::discover(&dirs.config);
        let theme = resolve_theme(&themes, cli.theme.or(file.app.theme), &dirs)?;

        Ok(Self {
            dirs,
            app,
            logging,
            telegram,
            tdlib_log,
            keymap,
            theme,
            themes,
            command: cli.command,
        })
    }

    /// Defaults only: no config files, environment or CLI values.
    #[cfg(test)]
    pub(crate) fn for_tests() -> Self {
        Self::resolve(CliOverrides::default(), |key| {
            (key == "HOME").then(|| "/nonexistent/tgt-test-home".into())
        })
        .expect("default settings resolve")
    }

    /// Loads a theme by name from [`Settings::themes`].
    pub fn load_theme(&self, name: &str) -> Result<Theme, SettingsError> {
        load_theme(&self.themes, name)
    }
}

fn resolve_app(file: &FileConfig) -> Result<App, SettingsError> {
    let app = &file.app;
    let frame_rate = app.frame_rate.unwrap_or(DEFAULT_FRAME_RATE);
    if frame_rate <= 0.0 || !frame_rate.is_finite() {
        return Err(SettingsError::FrameRate);
    }
    Ok(App {
        mouse_support: app.mouse_support.unwrap_or(true),
        paste_support: app.paste_support.unwrap_or(true),
        frame_rate,
        show_status_bar: app.show_status_bar.unwrap_or(true),
        show_title_bar: app.show_title_bar.unwrap_or(true),
        theme_enable: app.theme_enable.unwrap_or(true),
        use_emoji_icons: app.use_emoji_icons.unwrap_or(false),
        photo_max_dimension: app
            .photo_max_dimension
            .unwrap_or(DEFAULT_PHOTO_MAX_DIMENSION),
        photo_viewer_popup_size: app
            .photo_viewer_popup_size
            .unwrap_or(DEFAULT_PHOTO_VIEWER_POPUP_SIZE)
            .clamp(0.01, 1.0),
    })
}

fn resolve_telegram<F, P>(
    file: &FileConfig,
    dirs: &Dirs,
    command: &Command,
    lookup: &F,
    from_config: &P,
) -> Result<TdParams, SettingsError>
where
    F: Fn(&str) -> Option<OsString>,
    P: Fn(&'static str, Option<PathBuf>) -> Result<Option<PathBuf>, SettingsError>,
{
    let tg = &file.telegram;
    let api_id = match lookup(API_ID_ENV) {
        Some(value) => {
            let value = value.to_string_lossy();
            value
                .trim()
                .parse()
                .map_err(|_| SettingsError::ApiId(value.into_owned()))?
        }
        None => tg.api_id.unwrap_or(DEFAULT_API_ID),
    };
    let api_hash = lookup(API_HASH_ENV)
        .map(|value| value.to_string_lossy().trim().to_owned())
        .or_else(|| tg.api_hash.clone())
        .unwrap_or_else(|| DEFAULT_API_HASH.to_owned());

    // Without the message database, messages deleted elsewhere can't be resent
    // by a one-shot `tgt send`.
    let use_message_database =
        !matches!(command, Command::Send { .. }) && tg.use_message_database.unwrap_or(true);

    Ok(TdParams {
        api_id,
        api_hash,
        database_dir: from_config("telegram.database_dir", tg.database_dir.clone())?
            .unwrap_or_else(|| dirs.data.join("tdlib")),
        use_file_database: tg.use_file_database.unwrap_or(true),
        use_chat_info_database: tg.use_chat_info_database.unwrap_or(true),
        use_message_database,
        system_language_code: tg
            .system_language_code
            .clone()
            .unwrap_or_else(|| "en".into()),
        device_model: tg.device_model.clone().unwrap_or_else(|| "Desktop".into()),
        application_version: env!("CARGO_PKG_VERSION").to_owned(),
        proxy: tg.proxy.as_ref().and_then(resolve_proxy),
    })
}

/// A proxy section without a server is ignored.
fn resolve_proxy(proxy: &ProxyFile) -> Option<Proxy> {
    let server = proxy.server.clone().filter(|server| !server.is_empty())?;
    let username = proxy.username.clone().unwrap_or_default();
    let password = proxy.password.clone().unwrap_or_default();
    let kind = match proxy.r#type.unwrap_or_default() {
        ProxyType::Socks5 => ProxyKind::Socks5 { username, password },
        ProxyType::Http => ProxyKind::Http {
            username,
            password,
            http_only: proxy.http_only.unwrap_or(false),
        },
        ProxyType::Mtproto => ProxyKind::Mtproto {
            secret: proxy.secret.clone().unwrap_or_default(),
        },
    };
    Some(Proxy {
        server,
        port: proxy.port.unwrap_or(DEFAULT_PROXY_PORT),
        kind,
    })
}

/// An explicit theme (CLI or config) must exist; the remembered one falls
/// back to the default when its file is gone.
fn resolve_theme(
    themes: &[(String, ThemeSource)],
    explicit: Option<String>,
    dirs: &Dirs,
) -> Result<Theme, SettingsError> {
    if let Some(name) = explicit {
        return load_theme(themes, &name);
    }
    let remembered = std::fs::read_to_string(dirs.theme_state_file())
        .ok()
        .map(|name| name.trim().to_owned())
        .filter(|name| themes.iter().any(|(theme, _)| theme == name));
    match remembered {
        Some(name) => load_theme(themes, &name),
        None if themes.iter().any(|(name, _)| name == DEFAULT_THEME) => {
            load_theme(themes, DEFAULT_THEME)
        }
        None => Ok(Theme::default()),
    }
}

pub fn load_theme(themes: &[(String, ThemeSource)], name: &str) -> Result<Theme, SettingsError> {
    let (_, source) = themes
        .iter()
        .find(|(theme, _)| theme == name)
        .ok_or_else(|| ThemeError::Unknown(name.to_owned()))?;
    Ok(Theme::resolve(name, source.load()?)?)
}

/// CLI paths are relative to the working directory.
fn absolute(path: PathBuf) -> Result<PathBuf, SettingsError> {
    std::path::absolute(&path).map_err(|source| SettingsError::Absolute { path, source })
}

/// Config paths expand `~` and must then be absolute; empty means unset.
fn config_path(
    key: &'static str,
    path: Option<PathBuf>,
    home: &Path,
) -> Result<Option<PathBuf>, SettingsError> {
    let Some(path) = path.filter(|path| !path.as_os_str().is_empty()) else {
        return Ok(None);
    };

    let expanded = utils::expand_tilde(&path, home);
    if expanded.is_absolute() {
        Ok(Some(expanded))
    } else {
        Err(SettingsError::RelativePath { key, path })
    }
}
