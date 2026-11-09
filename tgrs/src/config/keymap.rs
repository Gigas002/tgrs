use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{ConfigError, read_toml};

pub const FILE_NAME: &str = "keymap.toml";

/// The default keymap; user files only need the bindings they change.
pub const BUNDLED: &str = include_str!("../../../examples/keymap.toml");

/// One binding: a key sequence and the command it runs.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct KeymapEntry {
    pub keys: Vec<String>,
    pub command: String,
    pub description: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct KeymapMode {
    #[serde(default)]
    pub keymap: Vec<KeymapEntry>,
}

/// Raw contents of `keymap.toml`, one section per UI component.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct KeymapFile {
    /// Bindings usable in every component.
    pub core_window: Option<KeymapMode>,
    pub chat_list: Option<KeymapMode>,
    pub archive_list: Option<KeymapMode>,
    pub topic_list: Option<KeymapMode>,
    pub chat: Option<KeymapMode>,
    pub prompt: Option<KeymapMode>,
    pub command_guide: Option<KeymapMode>,
    pub search_overlay: Option<KeymapMode>,
    pub message_actions: Option<KeymapMode>,
    pub file_upload_explorer: Option<KeymapMode>,
    pub pinned_messages_popup: Option<KeymapMode>,
    pub picker: Option<KeymapMode>,
    pub sticker_picker: Option<KeymapMode>,
    pub chat_actions: Option<KeymapMode>,
    pub profile: Option<KeymapMode>,
    pub find_chat: Option<KeymapMode>,
    pub edit_profile: Option<KeymapMode>,
    pub report: Option<KeymapMode>,
    pub blocked_list: Option<KeymapMode>,
    pub sessions: Option<KeymapMode>,
    pub forward: Option<KeymapMode>,
    pub downloads: Option<KeymapMode>,
    pub storage: Option<KeymapMode>,
    pub notification_settings: Option<KeymapMode>,
    pub privacy: Option<KeymapMode>,
    pub leader_menu: Option<KeymapMode>,
}

/// Loads a `keymap.toml` file; a missing file is `Ok(None)`.
pub fn load(path: &Path) -> Result<Option<KeymapFile>, ConfigError> {
    read_toml(path)
}

/// The bundled default keymap.
pub fn bundled() -> KeymapFile {
    toml::from_str(BUNDLED).expect("bundled keymap.toml is valid")
}
