//! XDG Base Directory resolution and small path helpers.
//!
//! Everything here is pure: environment values come in through a lookup
//! function, so callers decide where they are read from.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

#[cfg(test)]
mod tests;

/// Directory name used under every XDG base directory.
pub const APP_NAME: &str = "tgt";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PathsError {
    #[error("HOME is not set or is not an absolute path")]
    NoHome,
}

/// XDG base directories, with the spec's fallbacks applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XdgDirs {
    pub home: PathBuf,
    /// `$XDG_CONFIG_HOME`, fallback `~/.config`.
    pub config_home: PathBuf,
    /// `$XDG_DATA_HOME`, fallback `~/.local/share`.
    pub data_home: PathBuf,
    /// `$XDG_STATE_HOME`, fallback `~/.local/state`.
    pub state_home: PathBuf,
    /// `$XDG_DOWNLOAD_DIR`, fallback `~/Downloads`.
    pub download_dir: PathBuf,
}

impl XdgDirs {
    /// Resolves base directories from an environment lookup, e.g. `std::env::var_os`.
    ///
    /// Per the spec, empty or relative values are ignored and the fallback is used.
    pub fn from_lookup<F>(lookup: &F) -> Result<Self, PathsError>
    where
        F: Fn(&str) -> Option<OsString>,
    {
        let home = absolute_var(lookup, "HOME").ok_or(PathsError::NoHome)?;
        let base = |var, fallback: &str| absolute_var(lookup, var).unwrap_or(home.join(fallback));

        Ok(Self {
            config_home: base("XDG_CONFIG_HOME", ".config"),
            data_home: base("XDG_DATA_HOME", ".local/share"),
            state_home: base("XDG_STATE_HOME", ".local/state"),
            download_dir: base("XDG_DOWNLOAD_DIR", "Downloads"),
            home,
        })
    }
}

/// Default tgt directories derived from [`XdgDirs`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppDirs {
    /// `config.toml`, `keymap.toml` and `theme.toml`.
    pub config: PathBuf,
    /// TDLib database and downloaded files.
    pub data: PathBuf,
    /// Logs.
    pub state: PathBuf,
}

impl AppDirs {
    pub fn new(xdg: &XdgDirs) -> Self {
        Self {
            config: xdg.config_home.join(APP_NAME),
            data: xdg.data_home.join(APP_NAME),
            state: xdg.state_home.join(APP_NAME),
        }
    }
}

/// Expands a leading `~` (alone or followed by `/`) to `home`.
///
/// `~user` forms are left untouched.
pub fn expand_tilde(path: &Path, home: &Path) -> PathBuf {
    match path.strip_prefix("~") {
        Ok(rest) => home.join(rest),
        Err(_) => path.to_path_buf(),
    }
}

fn absolute_var<F>(lookup: &F, var: &str) -> Option<PathBuf>
where
    F: Fn(&str) -> Option<OsString>,
{
    lookup(var)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}
