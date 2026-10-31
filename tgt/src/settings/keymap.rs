use std::{
    collections::{HashMap, HashSet, hash_map::Entry},
    str::FromStr,
};

use crossterm::event::{KeyCode, KeyModifiers};

use crate::{
    app::{action::Action, event::Event},
    config::keymap::{KeymapEntry, KeymapFile, KeymapMode},
    ui::component_name::ComponentName,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionBinding {
    /// A key (with modifiers) bound to an action.
    Single {
        action: Action,
        description: Option<String>,
    },
    /// The first key of a multi-key sequence.
    Multiple(HashMap<Event, ActionBinding>),
}

pub type KeyMap = HashMap<Event, ActionBinding>;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum KeymapError {
    #[error("keymap.toml [{section}]: unknown key {key:?}")]
    UnknownKey { section: &'static str, key: String },

    #[error("keymap.toml [{section}]: unknown command {command:?}")]
    UnknownCommand {
        section: &'static str,
        command: String,
    },
}

/// Resolved key bindings: bundled defaults merged with the user's `keymap.toml`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Keymap {
    /// Bindings usable in every component.
    core_window: KeyMap,
    /// Each component's own section, without `core_window`.
    own: HashMap<ComponentName, KeyMap>,
    /// `core_window` plus the component's own bindings, which take precedence.
    merged: HashMap<ComponentName, KeyMap>,
}

/// Components with their own section in `keymap.toml`.
const SECTIONS: &[(ComponentName, &str)] = &[
    (ComponentName::ChatList, "chat_list"),
    (ComponentName::TopicList, "topic_list"),
    (ComponentName::Chat, "chat"),
    (ComponentName::Prompt, "prompt"),
    (ComponentName::CommandGuide, "command_guide"),
    (ComponentName::SearchOverlay, "search_overlay"),
    (ComponentName::MessageActions, "message_actions"),
    (ComponentName::FileUploadExplorer, "file_upload_explorer"),
    (ComponentName::PinnedMessagesPopup, "pinned_messages_popup"),
    (ComponentName::Picker, "picker"),
    (ComponentName::LeaderMenu, "leader_menu"),
];

fn section(file: &KeymapFile, component: ComponentName) -> Option<&KeymapMode> {
    match component {
        ComponentName::ChatList => file.chat_list.as_ref(),
        ComponentName::TopicList => file.topic_list.as_ref(),
        ComponentName::Chat => file.chat.as_ref(),
        ComponentName::Prompt => file.prompt.as_ref(),
        ComponentName::CommandGuide => file.command_guide.as_ref(),
        ComponentName::SearchOverlay => file.search_overlay.as_ref(),
        ComponentName::MessageActions => file.message_actions.as_ref(),
        ComponentName::FileUploadExplorer => file.file_upload_explorer.as_ref(),
        ComponentName::PinnedMessagesPopup => file.pinned_messages_popup.as_ref(),
        ComponentName::Picker => file.picker.as_ref(),
        ComponentName::LeaderMenu => file.leader_menu.as_ref(),
        _ => file.core_window.as_ref(),
    }
}

impl Keymap {
    /// Merges `user` over `default`: the user's entries win, and every default
    /// command the user didn't bind in that section is kept.
    pub fn resolve(default: &KeymapFile, user: Option<&KeymapFile>) -> Result<Self, KeymapError> {
        let entries = |component| {
            merge_entries(
                section(default, component),
                user.and_then(|u| section(u, component)),
            )
        };

        let core_window = to_map(&entries(ComponentName::CoreWindow), "core_window")?;
        let mut own_sections = HashMap::new();
        let mut merged = HashMap::new();
        let mut seen: HashSet<Event> = core_window.keys().cloned().collect();

        for (component, name) in SECTIONS {
            let own = to_map(&entries(*component), name)?;
            for key in own.keys() {
                if !seen.insert(key.clone()) {
                    tracing::debug!("keymap.toml [{name}]: {key} is also bound in another section");
                }
            }

            let mut map = core_window.clone();
            if matches!(
                component,
                ComponentName::Prompt | ComponentName::SearchOverlay
            ) {
                // These take printable keys as text, so inherited plain keys
                // (`q` to quit, Space for the leader menu) must not swallow
                // them. Explicit bindings of their sections are added after.
                map.retain(|key, _| !is_plain_char(key));
            }
            map.extend(own.clone());
            merged.insert(*component, map);
            own_sections.insert(*component, own);
        }

        Ok(Self {
            core_window,
            own: own_sections,
            merged,
        })
    }

    /// Bindings usable in every component.
    pub fn core_window(&self) -> &KeyMap {
        &self.core_window
    }

    /// Only the bindings from a component's own section.
    pub fn section(&self, component: ComponentName) -> Option<&KeyMap> {
        self.own.get(&component)
    }

    /// The effective bindings for a component: `core_window` plus its own section.
    pub fn get_map_of(&self, component: Option<ComponentName>) -> &KeyMap {
        component
            .and_then(|component| self.merged.get(&component))
            .unwrap_or(&self.core_window)
    }

    /// Keys bound directly (not as a sequence) to `value` in a component.
    pub fn get_key_of_single_action(&self, component: ComponentName, value: Action) -> Vec<Event> {
        self.get_map_of(Some(component))
            .iter()
            .filter_map(|(key, binding)| match binding {
                ActionBinding::Single { action, .. } if *action == value => Some(key.clone()),
                _ => None,
            })
            .collect()
    }
}

/// A printable key without Ctrl/Alt, which text inputs take as text.
pub fn is_plain_char(key: &Event) -> bool {
    matches!(key, Event::Key(KeyCode::Char(_), modifiers)
        if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER))
}

/// User entries first, then default entries whose command the user didn't bind.
fn merge_entries(default: Option<&KeymapMode>, user: Option<&KeymapMode>) -> Vec<KeymapEntry> {
    let user = user.map(|mode| mode.keymap.as_slice()).unwrap_or_default();
    let bound: HashSet<&str> = user.iter().map(|e| e.command.as_str()).collect();
    let mut entries = user.to_vec();
    entries.extend(
        default
            .map(|mode| mode.keymap.as_slice())
            .unwrap_or_default()
            .iter()
            .filter(|e| !bound.contains(e.command.as_str()))
            .cloned(),
    );
    entries
}

fn to_map(entries: &[KeymapEntry], section: &'static str) -> Result<KeyMap, KeymapError> {
    let mut map = KeyMap::new();
    for entry in entries {
        let keys = entry
            .keys
            .iter()
            .map(|key| {
                Event::from_str(key).map_err(|_| KeymapError::UnknownKey {
                    section,
                    key: key.clone(),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let action = Action::from_str(&entry.command).map_err(|_| KeymapError::UnknownCommand {
            section,
            command: entry.command.clone(),
        })?;
        if !insert(&mut map, &keys, action, entry.description.clone()) {
            // The first binding of a key wins; user entries come first.
            tracing::debug!("keymap.toml [{section}]: {:?} is already bound", entry.keys);
        }
    }
    Ok(map)
}

/// Inserts a key sequence; `false` when it collides with an existing binding
/// (a key can't be both a binding and the start of a sequence).
fn insert(map: &mut KeyMap, keys: &[Event], action: Action, description: Option<String>) -> bool {
    match keys {
        [] => true,
        [key] => match map.entry(key.clone()) {
            Entry::Occupied(_) => false,
            Entry::Vacant(slot) => {
                slot.insert(ActionBinding::Single {
                    action,
                    description,
                });
                true
            }
        },
        [key, rest @ ..] => match map
            .entry(key.clone())
            .or_insert_with(|| ActionBinding::Multiple(KeyMap::new()))
        {
            ActionBinding::Multiple(next) => insert(next, rest, action, description),
            ActionBinding::Single { .. } => false,
        },
    }
}
