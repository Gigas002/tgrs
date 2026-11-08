//! Popup showing how much disk space Telegram's files and database take,
//! with the chats that have the most files, to clear the cache.

use crate::{
    app::action::Action,
    app::context::AppContext,
    ui::component_name::ComponentName,
    ui::component_traits::{Component, HandleFocus},
    ui::popup_list::{self, PopupRow},
};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use libtgrs::model::{ChatStorage, StorageUsage, format_size};
use ratatui::{Frame, layout::Rect, text::Span};
use std::{io, sync::Arc};
use tokio::sync::mpsc::UnboundedSender;

/// What a second press of the same key will delete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Armed {
    All,
    Chat(i64),
}

pub struct Storage {
    app_context: Arc<AppContext>,
    name: String,
    action_tx: Option<UnboundedSender<Action>>,
    focused: bool,
    visible: bool,
    /// None while loading.
    usage: Option<StorageUsage>,
    /// The highlighted chat.
    selected: usize,
    armed: Option<Armed>,
    /// Popup bounds and first visible row from the most recent draw.
    last_popup: Option<(Rect, usize)>,
}

impl Storage {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Storage {
            app_context,
            name: String::new(),
            action_tx: None,
            focused: false,
            visible: false,
            usage: None,
            selected: 0,
            armed: None,
            last_popup: None,
        }
    }

    pub fn with_name(mut self, name: impl AsRef<str>) -> Self {
        self.name = name.as_ref().to_string();
        self
    }

    fn send(&self, action: Action) {
        if let Some(tx) = self.action_tx.as_ref() {
            let _ = tx.send(action);
        }
    }

    fn chats(&self) -> &[ChatStorage] {
        self.usage
            .as_ref()
            .map_or(&[], |usage| usage.chats.as_slice())
    }

    fn move_selection(&mut self, forward: bool) {
        let len = self.chats().len();
        if len > 0 {
            self.armed = None;
            self.selected = if forward {
                (self.selected + 1) % len
            } else {
                (self.selected + len - 1) % len
            };
        }
    }

    /// Deletes every cached file on the second press.
    fn clear_all(&mut self) {
        let Some(usage) = self.usage.as_ref() else {
            return;
        };
        if usage.files_size == 0 {
            self.armed = None;
            self.send(Action::StatusMessage("No cached files.".into()));
            return;
        }
        if self.armed != Some(Armed::All) {
            self.armed = Some(Armed::All);
            return;
        }
        self.armed = None;
        self.usage = None;
        self.send(Action::ClearCache(None));
    }

    /// Deletes the highlighted chat's cached files on the second press.
    fn clear_chat(&mut self) {
        let Some(chat_id) = self.chats().get(self.selected).map(|chat| chat.chat_id) else {
            return;
        };
        if self.armed != Some(Armed::Chat(chat_id)) {
            self.armed = Some(Armed::Chat(chat_id));
            return;
        }
        self.armed = None;
        self.usage = None;
        self.send(Action::ClearCache(Some(chat_id)));
    }

    /// The shortest key bound to `action` in this popup.
    fn key_of(&self, action: Action) -> Option<String> {
        let mut keys = self
            .app_context
            .keymap_config()
            .get_key_of_single_action(ComponentName::Storage, action);
        keys.sort_by_key(|key| key.to_string().len());
        keys.first().map(ToString::to_string)
    }

    fn title(&self) -> String {
        let again = |key: Option<String>, what: &str| match key {
            Some(key) => format!("Press {key} again to {what}"),
            None => format!("Press again to {what}"),
        };
        match self.armed {
            Some(Armed::All) => again(
                self.key_of(Action::StorageClearAll),
                "delete all cached files",
            ),
            Some(Armed::Chat(_)) => again(
                self.key_of(Action::StorageClearChat),
                "delete this chat's files",
            ),
            None => {
                let mut title = match self.usage.as_ref() {
                    Some(usage) => format!("Storage · {}", format_size(usage.total())),
                    None => "Storage".to_owned(),
                };
                if self.usage.is_some() {
                    if let Some(key) = self.key_of(Action::StorageClearAll) {
                        title.push_str(&format!(" · {key} clear all"));
                    }
                    if !self.chats().is_empty()
                        && let Some(key) = self.key_of(Action::StorageClearChat)
                    {
                        title.push_str(&format!(" · {key} clear chat"));
                    }
                }
                title
            }
        }
    }

    #[cfg(test)]
    fn chat_ids(&self) -> Vec<i64> {
        self.chats().iter().map(|chat| chat.chat_id).collect()
    }
}

impl HandleFocus for Storage {
    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl Component for Storage {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> io::Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn update(&mut self, action: Action) {
        match action {
            Action::ShowStorage => {
                self.visible = true;
                self.usage = None;
                self.selected = 0;
                self.armed = None;
            }
            Action::HideStorage => {
                self.visible = false;
                self.usage = None;
                self.armed = None;
            }
            Action::StorageLoaded(usage) if self.visible => {
                self.selected = self.selected.min(usage.chats.len().saturating_sub(1));
                self.usage = Some(usage);
                self.armed = None;
            }
            Action::StorageNext => self.move_selection(true),
            Action::StoragePrevious => self.move_selection(false),
            Action::StorageClearAll => self.clear_all(),
            Action::StorageClearChat => self.clear_chat(),
            _ => {}
        }
    }

    fn handle_mouse_events(&mut self, mouse: MouseEvent) -> io::Result<Option<Action>> {
        let Some((popup, offset)) = self.last_popup else {
            return Ok(None);
        };
        match mouse.kind {
            MouseEventKind::ScrollUp => self.move_selection(false),
            MouseEventKind::ScrollDown => self.move_selection(true),
            MouseEventKind::Down(MouseButton::Left) => {
                let inside = mouse.column >= popup.x
                    && mouse.column < popup.right()
                    && mouse.row >= popup.y
                    && mouse.row < popup.bottom();
                if !inside {
                    return Ok(Some(Action::HideStorage));
                }
                // The first row is the totals
                if mouse.row > popup.y && mouse.row + 1 < popup.bottom() {
                    let row = offset + usize::from(mouse.row - popup.y - 1);
                    if let Some(index) = row.checked_sub(1)
                        && index < self.chats().len()
                        && index != self.selected
                    {
                        self.selected = index;
                        self.armed = None;
                    }
                }
            }
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect) -> io::Result<()> {
        if !self.visible {
            return Ok(());
        }
        let theme = self.app_context.theme();
        let info = |text: String| PopupRow {
            key: String::new(),
            label: vec![Span::raw(text)],
            disabled: true,
        };
        let rows: Vec<PopupRow> = match self.usage.as_ref() {
            None => vec![info("Loading…".to_owned())],
            Some(usage) => std::iter::once(info(usage.summary()))
                .chain(usage.chats.iter().map(|chat| PopupRow {
                    key: String::new(),
                    label: vec![Span::raw(chat.summary())],
                    disabled: false,
                }))
                .collect(),
        };
        let selected = if self.chats().is_empty() {
            usize::MAX
        } else {
            self.selected + 1
        };
        self.last_popup = Some(popup_list::draw(
            frame,
            area,
            theme,
            &self.title(),
            &rows,
            selected,
            60,
        ));
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/storage.rs"]
mod tests;
