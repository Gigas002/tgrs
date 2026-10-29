use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use super::{BUNDLED_FILES, ConfigError, read_toml};

pub const DIR_NAME: &str = "themes";

/// One style entry. Colors are palette names, color names, `#rgb`/`#rrggbb`,
/// `r,g,b` or an ANSI index; an empty string means "unset".
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct ThemeEntry {
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub italic: Option<bool>,
    pub bold: Option<bool>,
    pub underline: Option<bool>,
}

pub type ThemeSection = HashMap<String, ThemeEntry>;

/// Raw contents of a theme file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct ThemeFile {
    pub palette: HashMap<String, String>,
    pub common: ThemeSection,
    pub chat_list: ThemeSection,
    pub chat: ThemeSection,
    pub prompt: ThemeSection,
    pub status_bar: ThemeSection,
    pub title_bar: ThemeSection,
    pub reply_message: ThemeSection,
}

/// Where a theme comes from: a file in the user's `themes/` or one bundled in the binary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThemeSource {
    File(PathBuf),
    Bundled(&'static str),
}

impl ThemeSource {
    pub fn load(&self) -> Result<ThemeFile, ConfigError> {
        match self {
            Self::File(path) => Ok(read_toml(path)?.unwrap_or_default()),
            Self::Bundled(content) => {
                toml::from_str(content).map_err(|source| ConfigError::Parse {
                    path: PathBuf::from("<bundled theme>"),
                    source,
                })
            }
        }
    }
}

/// Themes available to `config_dir`: user files in `themes/` shadow bundled ones,
/// sorted by name with the default `theme` first.
pub fn discover(config_dir: &Path) -> Vec<(String, ThemeSource)> {
    let mut themes: Vec<(String, ThemeSource)> = BUNDLED_FILES
        .iter()
        .filter_map(|(path, content)| {
            let name = path.strip_prefix("themes/")?.strip_suffix(".toml")?;
            Some((name.to_owned(), ThemeSource::Bundled(content)))
        })
        .collect();

    if let Ok(entries) = std::fs::read_dir(config_dir.join(DIR_NAME)) {
        for path in entries.flatten().map(|entry| entry.path()) {
            let Some(name) = path
                .extension()
                .filter(|ext| *ext == "toml")
                .and(path.file_stem())
                .and_then(|stem| stem.to_str())
                .map(str::to_owned)
            else {
                continue;
            };
            themes.retain(|(existing, _)| *existing != name);
            themes.push((name, ThemeSource::File(path)));
        }
    }

    themes.sort_by(|(a, _), (b, _)| (a != "theme", a).cmp(&(b != "theme", b)));
    themes
}
