use crate::{
    app::action::{Action, MessageCommand},
    app::context::AppContext,
    ui::component_name::ComponentName,
    ui::component_traits::{Component, HandleFocus},
    ui::message,
};
use arboard::Clipboard;
use crossterm::event::{KeyCode, MouseEventKind};
use libtgt::model::MessageEntry;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
    symbols::{
        border::{self, Set},
        line,
    },
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListDirection, ListItem, ListState, Paragraph},
};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

/// `ChatWindow` is a struct that represents a window for displaying a chat.
/// It is responsible for managing the layout and rendering of the chat window.
pub struct ChatWindow {
    /// The application context.
    app_context: Arc<AppContext>,
    /// The name of the `ChatWindow`.
    name: String,
    /// An unbounded sender that send action for processing.
    action_tx: Option<UnboundedSender<Action>>,
    /// A list of message items to be displayed in the `ChatWindow`.
    message_list: Vec<MessageEntry>,
    /// The state of the list.
    message_list_state: ListState,
    /// Indicates whether the `ChatWindow` is focused or not.
    focused: bool,
    /// When true, next draw will select the newest message (Alt+C restore order).
    request_jump_to_latest: bool,
}
/// Implementation of the `ChatWindow` struct.
impl ChatWindow {
    /// Create a new instance of the `ChatWindow` struct.
    ///
    /// # Arguments
    /// * `app_context` - An Arc wrapped AppContext struct.
    ///
    /// # Returns
    /// * `Self` - The new instance of the `ChatWindow` struct.
    pub fn new(app_context: Arc<AppContext>) -> Self {
        let name = "".to_string();
        let action_tx = None;
        let message_list = vec![];
        let message_list_state = ListState::default();
        let focused = false;
        ChatWindow {
            app_context,
            name,
            action_tx,
            message_list,
            message_list_state,
            focused,
            request_jump_to_latest: false,
        }
    }
    /// Set the name of the `ChatWindow`.
    ///
    /// # Arguments
    /// * `name` - The name of the `ChatWindow`.
    ///
    /// # Returns
    /// * `Self` - The modified instance of the `ChatWindow`.
    pub fn with_name(mut self, name: impl AsRef<str>) -> Self {
        self.name = name.as_ref().to_string();
        self
    }

    /// Build message_list from data layer (read-only API). Uses a single-lock snapshot to avoid
    /// TOCTOU: another thread clearing the store between ordered_message_ids() and get_message().
    fn refresh_message_list_from_store(&mut self) {
        self.message_list = self.app_context.store().ordered_messages_snapshot();
    }

    fn pinned_preview_text(&self) -> String {
        let pins = self.app_context.store().open_chat_pinned_snapshot();
        let Some(latest) = pins.first() else {
            return String::new();
        };
        let preview = latest.pinned_strip_preview(200);
        if pins.len() > 1 {
            format!("{} · +{} pinned", preview, pins.len() - 1)
        } else {
            preview
        }
    }

    /// Chords from the merged `[chat]` keymap that open the pinned popup (shown on the pin strip).
    fn pinned_messages_popup_keys_hint(&self) -> String {
        use std::collections::BTreeSet;
        let keys = self
            .app_context
            .keymap_config()
            .get_key_of_single_action(ComponentName::Chat, Action::ShowPinnedMessagesPopup);
        let labels: BTreeSet<String> = keys.into_iter().map(|e| e.to_string()).collect();
        if labels.is_empty() {
            "Shift+Tab".to_string()
        } else {
            labels.into_iter().collect::<Vec<_>>().join(" / ")
        }
    }

    /// Select the next message item in the list (down = towards newer messages).
    fn next(&mut self) {
        let len = self.message_list.len();
        // Load more history when near top of loaded range (and not already loading)
        if len > 0 && !self.app_context.store().is_history_loading() {
            let oldest = self.app_context.store().oldest_message_id();
            let selected_id = self
                .message_list_state
                .selected()
                .and_then(|i| self.message_list.get(i).map(|m| m.id()));
            let near_top = match (oldest, selected_id) {
                (Some(old), Some(sel)) => sel == old,
                (_, Some(_)) => self.message_list_state.selected() == Some(0),
                _ => false,
            };
            if near_top {
                let _ = self.app_context.action_tx().send(Action::GetChatHistory);
            }
        }

        // Handle empty list: unselect and return early
        if len == 0 {
            self.message_list_state.select(None);
            return;
        }

        // Bounds check: saturating_sub prevents going below 0 when already at oldest message (index 0).
        // If no selection, start at index 0 (oldest message).
        // Without these checks, scrolling past the ends could cause panics or invalid indices.
        let i = self
            .message_list_state
            .selected()
            .map(|i| i.saturating_sub(1))
            .unwrap_or(0);
        self.message_list_state.select(Some(i));
    }

    /// Select the previous message item in the list (up = towards older messages).
    fn previous(&mut self) {
        let len = self.message_list.len();
        // Load newer messages when near bottom (so user can scroll forward in time)
        if len > 0 && !self.app_context.store().is_history_loading() {
            let newest = self.app_context.store().newest_message_id();
            let selected_id = self
                .message_list_state
                .selected()
                .and_then(|i| self.message_list.get(i).map(|m| m.id()));
            let near_bottom = match (newest, selected_id) {
                (Some(new), Some(sel)) => sel == new,
                (_, Some(_)) => self.message_list_state.selected() == Some(len.saturating_sub(1)),
                _ => false,
            };
            if near_bottom {
                let _ = self
                    .app_context
                    .action_tx()
                    .send(Action::GetChatHistoryNewer);
            }
        }

        // Handle empty list: unselect and return early
        if len == 0 {
            self.message_list_state.select(None);
            return;
        }

        // Bounds check: min(max_idx) prevents going above len-1 when already at newest message (index len-1).
        // If no selection, start at index 0 (oldest message).
        // Without these checks, scrolling past the ends could cause panics or invalid indices.
        let max_idx = len.saturating_sub(1);
        let i = self
            .message_list_state
            .selected()
            .map(|i| (i + 1).min(max_idx))
            .unwrap_or(0);
        self.message_list_state.select(Some(i));
    }

    /// Unselect the message item in the list.
    fn unselect(&mut self) {
        self.message_list_state.select(None);
    }

    /// The id of the selected message.
    fn selected_message_id(&self) -> Option<i64> {
        self.message_list_state
            .selected()
            .and_then(|i| self.message_list.get(i))
            .map(MessageEntry::id)
    }

    /// A message of the open chat, loaded or not (e.g. a pinned one).
    fn message(&self, message_id: i64) -> Option<MessageEntry> {
        self.message_list
            .iter()
            .find(|m| m.id() == message_id)
            .cloned()
            .or_else(|| self.app_context.store().get_message(message_id))
    }

    fn send(&self, action: Action) {
        if let Some(tx) = self.action_tx.as_ref() {
            let _ = tx.send(action);
        }
    }

    /// Runs `command` on the selected message.
    fn run_on_selected(&mut self, command: MessageCommand) {
        match self.selected_message_id() {
            Some(message_id) => self.run_command(command, message_id),
            None => self.send(Action::StatusMessage("Select a message first.".into())),
        }
    }

    fn run_command(&mut self, command: MessageCommand, message_id: i64) {
        let Some(entry) = self.message(message_id) else {
            return;
        };
        match command {
            MessageCommand::Reply => self.reply(&entry),
            MessageCommand::Edit => self.edit(&entry),
            MessageCommand::Copy => self.copy(&entry),
            MessageCommand::DeleteForEveryone => self.delete(&entry, true),
            MessageCommand::DeleteForMe => self.delete(&entry, false),
            MessageCommand::OpenMedia if entry.media().is_some() => {
                self.send(Action::OpenMedia(message_id));
            }
            MessageCommand::ViewMedia if entry.media().is_some() => {
                self.send(Action::ShowMediaViewer(message_id));
            }
            MessageCommand::DownloadMedia if entry.media().is_some() => {
                self.send(Action::DownloadMedia(message_id));
            }
            MessageCommand::OpenMedia
            | MessageCommand::ViewMedia
            | MessageCommand::DownloadMedia => {
                self.send(Action::StatusMessage("This message has no media.".into()));
            }
        }
    }

    /// Deletes one of our messages.
    ///
    /// # Arguments
    /// * `revoke` - A boolean flag indicating whether the message should be revoked or not.
    fn delete(&self, entry: &MessageEntry, revoke: bool) {
        if entry.sender_id() != self.app_context.store().me() {
            return;
        }
        let message_id = entry.id();
        self.send(Action::DeleteMessages(vec![message_id], revoke));
        self.app_context.store().delete_message(message_id);
    }

    /// Copies the text of a message to the clipboard.
    fn copy(&self, entry: &MessageEntry) {
        let message = entry.message_content_to_string();
        match Clipboard::new() {
            Ok(mut clipboard) => {
                if clipboard.set_text(&message).is_ok() {
                    self.send(Action::StatusMessage("Message yanked".into()));
                } else {
                    tracing::warn!("Clipboard set_text failed");
                }
            }
            Err(e) => {
                tracing::warn!("Clipboard unavailable (copy message): {}", e);
            }
        }
    }

    /// Edits a message. Only our own messages can be edited.
    fn edit(&self, entry: &MessageEntry) {
        if entry.sender_id() != self.app_context.store().me() {
            // Not our message: do nothing to avoid bugging the chat.
            return;
        }
        // The prompt must be focused first: actions go to the focused component.
        self.send(Action::FocusComponent(ComponentName::Prompt));
        self.send(Action::EditMessage(
            entry.id(),
            entry.message_content_to_string(),
        ));
    }

    /// Replies to a message.
    /// Focuses the prompt and sets reply mode so the user can type in the same prompt used for sending/editing.
    fn reply(&self, entry: &MessageEntry) {
        self.send(Action::FocusComponent(ComponentName::Prompt));
        self.send(Action::ReplyMessage(
            entry.id(),
            entry.message_content_to_string(),
        ));
    }

    /// Wraps each line with a border span on one side only (reply-target border-only highlight).
    /// Messages from others: `│` at the start of each line. Messages from me: `│` at the end.
    /// This keeps borders aligned and avoids broken vertical bars under each other.
    fn wrap_text_with_reply_border(
        content: Text,
        border_style: Style,
        alignment: Alignment,
        myself: bool,
    ) -> Text {
        let wrapped_lines: Vec<Line> = content
            .into_iter()
            .map(|line| {
                if myself {
                    let mut spans: Vec<Span> = line.into_iter().collect();
                    spans.push(Span::styled(" │", border_style));
                    Line::from(spans)
                } else {
                    let mut spans = vec![Span::styled("│ ", border_style)];
                    spans.extend(line);
                    Line::from(spans)
                }
            })
            .collect();
        Text::from(wrapped_lines).alignment(alignment)
    }
}

/// Implement the `HandleFocus` trait for the `ChatWindow` struct.
/// This trait allows the `ChatListWindow` to be focused or unfocused.
impl HandleFocus for ChatWindow {
    /// Set the `focused` flag for the `ChatWindow`.
    fn focus(&mut self) {
        self.focused = true;
    }
    /// Set the `focused` flag for the `ChatWindow`.
    fn unfocus(&mut self) {
        self.focused = false;
    }
}

/// Implement the `Component` trait for the `ChatListWindow` struct.
impl Component for ChatWindow {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> std::io::Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn handle_mouse_events(
        &mut self,
        mouse: crossterm::event::MouseEvent,
    ) -> std::io::Result<Option<Action>> {
        if !self.focused {
            return Ok(None);
        }
        match mouse.kind {
            MouseEventKind::ScrollDown => Ok(Some(Action::ChatWindowPrevious)),
            MouseEventKind::ScrollUp => Ok(Some(Action::ChatWindowNext)),
            _ => Ok(None),
        }
    }

    fn update(&mut self, action: Action) {
        match action {
            Action::ChatWindowNext => self.next(),
            Action::ChatWindowPrevious => self.previous(),
            Action::ChatWindowUnselect => self.unselect(),
            Action::SelectedMessage(command) => self.run_on_selected(command),
            Action::RunMessageCommand(command, message_id) => {
                self.run_command(command, message_id);
            }
            Action::ShowMessageActions => match self.selected_message_id() {
                Some(message_id) => self.send(Action::ShowMessageActionsFor(message_id)),
                None => self.send(Action::StatusMessage("Select a message first.".into())),
            },
            Action::JumpCompleted(_message_id) => {
                // Selection by message_id is applied in draw() when jump_target_message_id is set
            }
            Action::ChatWindowSortWithString(_) => {
                // No-op: chat message search is server-side only (search overlay)
            }
            Action::ChatWindowRestoreSort => {
                self.request_jump_to_latest = true;
            }
            Action::ChatWindowSearch | Action::ChatListSearch => {
                // Handled by CoreWindow: opens search overlay or focuses ChatList
            }
            Action::LoadPinnedMessages => {}
            Action::Key(key_code, modifiers) if self.focused => {
                let has_message_pins = !self
                    .app_context
                    .store()
                    .open_chat_pinned_snapshot()
                    .is_empty();
                match key_code {
                    KeyCode::Char('r') if modifiers.alt => {
                        // Alt+R: switch to ChatList search (handled by CoreWindow)
                        if let Some(tx) = self.action_tx.as_ref() {
                            let _ = tx.send(Action::ChatListSearch);
                        }
                    }
                    // Shift+Tab: open pinned popup when keymap misses modifier quirks.
                    KeyCode::BackTab if has_message_pins => {
                        if let Some(tx) = self.action_tx.as_ref() {
                            let _ = tx.send(Action::ShowPinnedMessagesPopup);
                        }
                    }
                    KeyCode::Tab if modifiers.shift && has_message_pins => {
                        if let Some(tx) = self.action_tx.as_ref() {
                            let _ = tx.send(Action::ShowPinnedMessagesPopup);
                        }
                    }
                    KeyCode::Up => self.next(),
                    KeyCode::Down => {
                        self.previous();
                    }
                    KeyCode::Tab if !modifiers.shift => {
                        self.next();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn draw(&mut self, frame: &mut ratatui::Frame<'_>, area: Rect) -> std::io::Result<()> {
        // Capture selection by ID before any clear, so we can restore viewport on redraw (e.g. when unfocused)
        let selected_message_id_before = self
            .message_list_state
            .selected()
            .and_then(|idx| self.message_list.get(idx).map(|m| m.id()));

        if !self.focused {
            self.message_list_state.select(None);
        }

        // Always refresh message list from store (no local filtering; search is server-side).
        let selected_message_id = selected_message_id_before;
        let prev_len = self.message_list.len();
        self.refresh_message_list_from_store();

        // After jump-to-message: select the target and clear the flag
        let jump_target = self.app_context.store().jump_target_message_id().as_i64();
        if jump_target != 0 {
            self.app_context.store().set_jump_target_message_id_i64(0);
            if let Some(idx) = self.message_list.iter().position(|m| m.id() == jump_target) {
                self.message_list_state.select(Some(idx));
            }
        } else {
            // Alt+C restore order: jump to latest message
            if self.request_jump_to_latest {
                self.request_jump_to_latest = false;
                if !self.message_list.is_empty()
                    && let Some(newest_id) = self.app_context.store().newest_message_id()
                    && let Some(new_idx) =
                        self.message_list.iter().position(|m| m.id() == newest_id)
                {
                    self.message_list_state.select(Some(new_idx));
                }
            } else {
                // Restore selection by message ID when possible
                let selection_restored = selected_message_id.and_then(|id| {
                    self.message_list
                        .iter()
                        .position(|m| m.id() == id)
                        .map(|idx| {
                            self.message_list_state.select(Some(idx));
                            id
                        })
                });
                // When we have no valid selection (e.g. just entered chat), jump to latest by ID.
                // When list grew and we were at bottom (selected was newest), stay at bottom.
                let at_bottom = selection_restored
                    .zip(self.app_context.store().newest_message_id())
                    .is_some_and(|(sel_id, newest_id)| sel_id == newest_id);
                let should_jump_to_latest = selection_restored.is_none()
                    || (at_bottom && prev_len < self.message_list.len());
                if should_jump_to_latest
                    && !self.message_list.is_empty()
                    && let Some(newest_id) = self.app_context.store().newest_message_id()
                    && let Some(new_idx) =
                        self.message_list.iter().position(|m| m.id() == newest_id)
                {
                    self.message_list_state.select(Some(new_idx));
                }
            }
        }

        let has_pins = !self
            .app_context
            .store()
            .open_chat_pinned_snapshot()
            .is_empty();
        // Fixed header + pinned section (label row + preview row); only the message list uses Min.
        let header_h = 3u16;
        let pin_h = if has_pins { 2u16 } else { 0u16 };
        let chat_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(header_h),
                Constraint::Length(pin_h),
                Constraint::Min(1),
            ])
            .split(area);

        let style_panel_border = if self.focused {
            self.app_context.style_border_component_focused()
        } else {
            self.app_context.style_chat()
        };

        let border = Set {
            top_left: line::NORMAL.vertical_right,
            top_right: line::NORMAL.vertical_left,
            bottom_left: line::NORMAL.horizontal_up,
            ..border::PLAIN
        };
        let block = Block::new()
            .border_set(border)
            .border_style(style_panel_border)
            .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
            .style(self.app_context.style_chat());
        let list_area = chat_layout[2];
        let list_inner = block.inner(list_area);
        // Leave margin so wrapped lines and wide chars don't overflow the right edge
        let wrap_width = list_inner.width.saturating_sub(2) as i32;

        let reply_message_id = self.app_context.store().reply_message_id().as_i64();
        let mut is_unread_outbox = true;
        let mut is_unread_inbox = true;
        let mut items: Vec<ListItem<'_>> = self
            .message_list
            .iter()
            .map(|message_entry| {
                let (myself, name_style, content_style, alignment) = if message_entry.sender_id()
                    == self.app_context.store().me()
                {
                    if message_entry.id() == self.app_context.store().last_read_outbox_message_id()
                    {
                        is_unread_outbox = false;
                    }
                    (
                        true,
                        self.app_context.style_chat_message_myself_name(),
                        self.app_context.style_chat_message_myself_content(),
                        Alignment::Right,
                    )
                } else {
                    if message_entry.id() == self.app_context.store().last_read_inbox_message_id() {
                        is_unread_inbox = false;
                    }
                    (
                        false,
                        self.app_context.style_chat_message_other_name(),
                        self.app_context.style_chat_message_other_content(),
                        Alignment::Left,
                    )
                };
                let content = message::message_text(
                    message_entry,
                    myself,
                    &self.app_context,
                    is_unread_outbox,
                    name_style,
                    content_style,
                    wrap_width,
                )
                .alignment(alignment);
                let is_reply_target =
                    reply_message_id != 0 && message_entry.id() == reply_message_id;
                if is_reply_target {
                    let border_style = self.app_context.style_item_reply_target();
                    let content_with_border =
                        Self::wrap_text_with_reply_border(content, border_style, alignment, myself);
                    // Do not apply border_style to the whole ListItem: only the │ spans
                    // use it, so the message keeps its original formatting (name, reply-to, content).
                    ListItem::new(content_with_border)
                } else {
                    ListItem::new(content)
                }
            })
            .collect();
        if self.app_context.store().is_history_loading() {
            items.push(ListItem::new(Line::from(Span::styled(
                "Loading…",
                self.app_context.style_timestamp(),
            ))));
        }

        // Render from bottom to top so content sits at the bottom (above the prompt) when
        // there are fewer messages than fit in the area. Reverse items so the list's
        // "first" item is newest; with BottomToTop that draws at the bottom, keeping
        // visual order: oldest at top, newest at bottom.
        items.reverse();
        let item_count = items.len();

        // Selection is stored as index into message_list (oldest=0). In reversed list,
        // newest=0, oldest=len-1. Convert to list index for the widget, then back after render.
        let orig_selected = self.message_list_state.selected();
        let list_selected = orig_selected.map(|i| item_count.saturating_sub(1).saturating_sub(i));
        self.message_list_state.select(list_selected);

        // Use normal selection highlight so message text keeps its original formatting.
        // Reply-target is already indicated by the │ border spans only.
        let highlight_style = self.app_context.style_item_selected();
        let list = List::new(items)
            .block(block)
            .style(self.app_context.style_chat())
            .highlight_style(highlight_style)
            .repeat_highlight_symbol(true)
            .direction(ListDirection::BottomToTop);

        let border_header = Set {
            top_left: line::NORMAL.horizontal_down,
            bottom_left: line::NORMAL.horizontal_up,
            ..border::PLAIN
        };
        let block_header = Block::new()
            .border_set(border_header)
            .border_style(style_panel_border)
            .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
            .style(self.app_context.style_chat())
            .title(self.name.as_str());
        let header = Paragraph::new(Line::from(vec![
            Span::styled(
                self.app_context
                    .store()
                    .name_of_open_chat_id()
                    .unwrap_or_default(),
                self.app_context.style_chat_chat_name(),
            ),
            Span::raw(" "),
            Span::styled(
                self.app_context.store().open_chat_user_status(),
                self.app_context.style_timestamp(),
            ),
        ]))
        .block(block_header)
        .alignment(Alignment::Center);

        frame.render_widget(header, chat_layout[0]);
        if pin_h > 0 {
            let pin_area = chat_layout[1];
            let pin_block = Block::new()
                .borders(Borders::LEFT | Borders::RIGHT)
                .border_style(style_panel_border)
                .style(self.app_context.style_chat());
            let pin_inner = pin_block.inner(pin_area);
            frame.render_widget(pin_block, pin_area);

            let pin_rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Length(1)])
                .split(pin_inner);

            let open_keys = self.pinned_messages_popup_keys_hint();
            let pin_title = Paragraph::new(Line::from(vec![Span::styled(
                format!("Pinned messages (press {open_keys} to view)"),
                self.app_context.style_chat_chat_name(),
            )]))
            .style(self.app_context.style_chat());

            let pin_preview = self.pinned_preview_text();
            let pin_line = Line::from(vec![
                Span::styled("📌 ", self.app_context.style_timestamp()),
                Span::styled(
                    if pin_preview.is_empty() {
                        "(no preview)".to_string()
                    } else {
                        pin_preview
                    },
                    self.app_context.style_chat_message_other_content(),
                ),
            ]);
            let pin_preview_para = Paragraph::new(pin_line).style(self.app_context.style_chat());

            frame.render_widget(pin_title, pin_rows[0]);
            frame.render_widget(pin_preview_para, pin_rows[1]);
        }
        frame.render_stateful_widget(list, list_area, &mut self.message_list_state);

        // Restore selection to message_list index (oldest=0) for next/previous and other logic.
        let list_sel = self.message_list_state.selected();
        self.message_list_state
            .select(list_sel.map(|i| item_count.saturating_sub(1).saturating_sub(i)));

        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/chat_window.rs"]
mod tests;
