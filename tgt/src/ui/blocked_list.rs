//! Popup listing the people and chats you've blocked, to unblock them.

use crate::{
    app::action::Action,
    app::context::AppContext,
    ui::component_name::ComponentName,
    ui::component_traits::{Component, HandleFocus},
    ui::popup_list::{self, PopupRow},
};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use libtgt::model::{BlockedSender, ChatKind, TdMessageSender};
use ratatui::{Frame, layout::Rect, text::Span};
use std::{io, sync::Arc};
use tokio::sync::mpsc::UnboundedSender;

pub struct BlockedList {
    app_context: Arc<AppContext>,
    name: String,
    action_tx: Option<UnboundedSender<Action>>,
    focused: bool,
    visible: bool,
    /// None while loading.
    entries: Option<Vec<BlockedSender>>,
    selected: usize,
    /// Popup bounds and first visible row from the most recent draw.
    last_popup: Option<(Rect, usize)>,
}

impl BlockedList {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        BlockedList {
            app_context,
            name: String::new(),
            action_tx: None,
            focused: false,
            visible: false,
            entries: None,
            selected: 0,
            last_popup: None,
        }
    }

    pub fn with_name(mut self, name: impl AsRef<str>) -> Self {
        self.name = name.as_ref().to_string();
        self
    }

    fn len(&self) -> usize {
        self.entries.as_ref().map_or(0, Vec::len)
    }

    fn move_selection(&mut self, forward: bool) {
        let len = self.len();
        if len > 0 {
            self.selected = if forward {
                (self.selected + 1) % len
            } else {
                (self.selected + len - 1) % len
            };
        }
    }

    /// Unblocks the highlighted entry; it leaves the list right away.
    fn unblock(&mut self) {
        let Some(entries) = self.entries.as_mut() else {
            return;
        };
        if self.selected >= entries.len() {
            return;
        }
        let entry = entries.remove(self.selected);
        self.selected = self.selected.min(entries.len().saturating_sub(1));
        if let Some(tx) = self.action_tx.as_ref() {
            let _ = tx.send(Action::UnblockSender(entry.sender));
        }
    }

    /// The key that unblocks, for the title.
    fn unblock_key(&self) -> Option<String> {
        let mut keys = self
            .app_context
            .keymap_config()
            .get_key_of_single_action(ComponentName::BlockedList, Action::BlockedListUnblock);
        keys.sort_by_key(|key| key.to_string().len());
        keys.first().map(ToString::to_string)
    }

    fn title(&self) -> String {
        let count = match self.entries.as_ref() {
            Some(entries) if !entries.is_empty() => format!(" ({})", entries.len()),
            _ => String::new(),
        };
        match self.unblock_key() {
            Some(key) if self.len() > 0 => format!("Blocked{count} · {key} unblocks"),
            _ => format!("Blocked{count}"),
        }
    }

    #[cfg(test)]
    fn names(&self) -> Vec<&str> {
        self.entries
            .iter()
            .flatten()
            .map(|entry| entry.name.as_str())
            .collect()
    }
}

impl HandleFocus for BlockedList {
    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl Component for BlockedList {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> io::Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn update(&mut self, action: Action) {
        match action {
            Action::ShowBlockedList => {
                self.visible = true;
                self.entries = None;
                self.selected = 0;
            }
            Action::HideBlockedList => {
                self.visible = false;
                self.entries = None;
            }
            Action::BlockedListLoaded(entries) if self.visible => {
                self.selected = self.selected.min(entries.len().saturating_sub(1));
                self.entries = Some(entries);
            }
            Action::BlockedListNext => self.move_selection(true),
            Action::BlockedListPrevious => self.move_selection(false),
            Action::BlockedListUnblock => self.unblock(),
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
                    return Ok(Some(Action::HideBlockedList));
                }
                if mouse.row > popup.y && mouse.row + 1 < popup.bottom() {
                    let index = offset + usize::from(mouse.row - popup.y - 1);
                    if index < self.len() {
                        self.selected = index;
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
        let placeholder = |text: &'static str| PopupRow {
            key: String::new(),
            label: vec![Span::raw(text)],
            disabled: true,
        };
        let rows: Vec<PopupRow> = match self.entries.as_ref() {
            None => vec![placeholder("Loading…")],
            Some(entries) if entries.is_empty() => vec![placeholder("No one is blocked")],
            Some(entries) => entries
                .iter()
                .map(|entry| {
                    let icon = match entry.sender {
                        TdMessageSender::User(_) => ChatKind::Private.icon(),
                        TdMessageSender::Chat(_) => ChatKind::Channel.icon(),
                    };
                    PopupRow {
                        key: String::new(),
                        label: vec![Span::raw(format!("{icon} {}", entry.name))],
                        disabled: false,
                    }
                })
                .collect(),
        };
        let selected = if self.len() > 0 {
            self.selected
        } else {
            usize::MAX
        };
        self.last_popup = Some(popup_list::draw(
            frame,
            area,
            theme,
            &self.title(),
            &rows,
            selected,
            36,
        ));
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/blocked_list.rs"]
mod tests;
