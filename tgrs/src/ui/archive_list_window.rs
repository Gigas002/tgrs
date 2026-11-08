//! The archived chats, in a pane right of the chat list.

use crate::app::action::Action;
use crate::app::context::AppContext;
use crate::settings::HighlightGroup;
use crate::ui::chat_actions::{chat_command_action, move_pinned_action};
use crate::ui::chat_list_window::{CHAT_LIST_ITEM_ROWS, chat_list_item_text, open_chat};
use crate::ui::component_name::ComponentName;
use crate::ui::component_traits::{Component, HandleFocus};
use crate::ui::pane;
use crossterm::event::{MouseButton, MouseEventKind};
use libtgt::model::ChatListEntry;
use libtgt::tdlib_rs::enums::ChatList;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{List, ListDirection, ListState, Paragraph};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

/// Archived chats loaded per request once the selection nears the end.
const LOAD_MORE: i32 = 20;

/// `ArchiveListWindow` lists the chats of the Archive chat list, opened from
/// the archive row of the chat list; opening one shows it in the chat.
pub struct ArchiveListWindow {
    /// The application context.
    app_context: Arc<AppContext>,
    /// The name of the `ArchiveListWindow`.
    name: String,
    /// An unbounded sender that send action for processing.
    command_tx: Option<UnboundedSender<Action>>,
    /// The chats shown, refreshed from the store before each use.
    visible_chats: Vec<ChatListEntry>,
    /// The selected chat, kept by id while chats move around.
    selected_chat_id: Option<i64>,
    /// The state of the list.
    chat_list_state: ListState,
    /// Indicates whether the `ArchiveListWindow` is focused or not.
    focused: bool,
    /// Where the rows were drawn last (for click-to-select item when focused).
    last_list_area: Option<Rect>,
    /// First visible item index after last draw (used for click-to-item).
    last_list_offset: Option<usize>,
}

impl ArchiveListWindow {
    /// Create a new instance of the `ArchiveListWindow` struct.
    pub fn new(app_context: Arc<AppContext>) -> Self {
        ArchiveListWindow {
            app_context,
            name: String::new(),
            command_tx: None,
            visible_chats: Vec::new(),
            selected_chat_id: None,
            chat_list_state: ListState::default(),
            focused: false,
            last_list_area: None,
            last_list_offset: None,
        }
    }

    /// Set the name of the `ArchiveListWindow`.
    pub fn with_name(mut self, name: impl AsRef<str>) -> Self {
        self.name = name.as_ref().to_string();
        self
    }

    /// Reloads the chats from the store. The selection stays on its chat,
    /// or goes to the open one, or the first.
    fn refresh(&mut self) {
        let store = self.app_context.store();
        self.visible_chats = store.archived_chat_list();
        let open_id = Some(store.open_chat_id().as_i64()).filter(|id| *id != 0);
        let index = [self.selected_chat_id, open_id]
            .into_iter()
            .flatten()
            .find_map(|id| self.visible_chats.iter().position(|c| c.chat_id == id))
            .or((!self.visible_chats.is_empty()).then_some(0));
        self.select(index);
    }

    fn select(&mut self, index: Option<usize>) {
        self.chat_list_state.select(index);
        self.selected_chat_id = index
            .and_then(|index| self.visible_chats.get(index))
            .map(|chat| chat.chat_id);
    }

    /// Moves the selection `step` chats, stopping at the ends; past the
    /// middle more archived chats are loaded.
    fn step(&mut self, step: isize) {
        self.refresh();
        if self.visible_chats.is_empty() {
            return;
        }
        let last = self.visible_chats.len() - 1;
        let index = self
            .chat_list_state
            .selected()
            .map_or(0, |index| index.saturating_add_signed(step).min(last));
        if index >= self.visible_chats.len() / 2 {
            let _ = self
                .app_context
                .action_tx()
                .send(Action::LoadChats(ChatList::Archive.into(), LOAD_MORE));
        }
        self.select(Some(index));
    }

    /// Opens the selected chat.
    fn confirm_selection(&mut self) {
        self.refresh();
        if let Some(chat) = self
            .chat_list_state
            .selected()
            .and_then(|index| self.visible_chats.get(index))
        {
            open_chat(&self.app_context, chat);
        }
    }
}

impl HandleFocus for ArchiveListWindow {
    fn focus(&mut self) {
        self.focused = true;
    }
    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl Component for ArchiveListWindow {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> std::io::Result<()> {
        self.command_tx = Some(tx);
        Ok(())
    }

    fn handle_mouse_events(
        &mut self,
        mouse: crossterm::event::MouseEvent,
    ) -> std::io::Result<Option<Action>> {
        match mouse.kind {
            MouseEventKind::ScrollDown if self.focused => Ok(Some(Action::ArchiveListNext)),
            MouseEventKind::ScrollUp if self.focused => Ok(Some(Action::ArchiveListPrevious)),
            // The first click focuses the pane (CoreWindow), the next opens a chat
            MouseEventKind::Down(MouseButton::Left) if self.focused => {
                let Some(area) = self.last_list_area else {
                    return Ok(None);
                };
                let offset = self.last_list_offset.unwrap_or(0);
                let index =
                    pane::item_at(area, mouse.column, mouse.row, offset, CHAT_LIST_ITEM_ROWS);
                match index {
                    Some(index) if index < self.visible_chats.len() => {
                        self.select(Some(index));
                        Ok(Some(Action::ArchiveListOpen))
                    }
                    _ => Ok(None),
                }
            }
            _ => Ok(None),
        }
    }

    fn update(&mut self, action: Action) {
        match action {
            Action::ArchiveListNext => self.step(1),
            Action::ArchiveListPrevious => self.step(-1),
            Action::ArchiveListOpen => self.confirm_selection(),
            Action::ShowChatActions | Action::SelectedChat(_) | Action::ShowProfile => {
                self.refresh();
                if let Some(chat_id) = self.selected_chat_id {
                    let _ = self.app_context.action_tx().send(match action {
                        Action::SelectedChat(command) => chat_command_action(command, chat_id),
                        Action::ShowProfile => Action::ShowProfileFor(chat_id),
                        _ => Action::ShowChatActionsFor(chat_id, None),
                    });
                }
            }
            // The selection stays on its chat, which moves
            Action::MovePinnedChat(up) => {
                self.refresh();
                if let Some(chat_id) = self.selected_chat_id {
                    let action = move_pinned_action(self.app_context.store(), chat_id, up);
                    let _ = self.app_context.action_tx().send(action);
                }
            }
            // Shown again: start at the open chat, or the first
            Action::FocusComponent(ComponentName::ArchiveList) => self.select(None),
            _ => {}
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect) -> std::io::Result<()> {
        let app_context = Arc::clone(&self.app_context);
        let theme = app_context.theme();
        self.refresh();

        let block = pane::list_block(theme, pane::title("Archive"), self.focused);
        let inner = block.inner(area);
        self.last_list_area = Some(inner);
        if self.visible_chats.is_empty() {
            let placeholder = Paragraph::new(Line::styled(
                "No archived chats",
                theme.style(HighlightGroup::Placeholder),
            ))
            .block(block);
            frame.render_widget(placeholder, area);
            return Ok(());
        }

        let width = usize::from(inner.width);
        let selected = self.chat_list_state.selected();
        let items = self.visible_chats.iter().enumerate().map(|(index, chat)| {
            chat_list_item_text(chat, &app_context, width, selected == Some(index))
        });
        let list = List::new(items)
            .block(block)
            .direction(ListDirection::TopToBottom);
        frame.render_stateful_widget(list, area, &mut self.chat_list_state);
        self.last_list_offset = Some(self.chat_list_state.offset());
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/archive_list_window.rs"]
mod tests;
