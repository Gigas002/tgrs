use crate::{
    app::action::{Action, MessageCommand, PickerKind},
    app::context::AppContext,
    settings::HighlightGroup,
    ui::component_name::ComponentName,
    ui::component_traits::{Component, HandleFocus},
    ui::message::{self, MessageRow},
    ui::pane,
};
use arboard::Clipboard;
use crossterm::event::{KeyCode, MouseEventKind};
use libtgt::model::{MessageEntry, TdMessageReplyTo};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{List, ListDirection, ListItem, ListState, Padding, Paragraph},
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
    /// Image previews shown under media messages.
    #[cfg(feature = "media-preview")]
    previews: crate::ui::preview::ChatPreviews,
}

/// A message's preview in the list: its message id, image, and the row in its
/// item where the preview starts.
#[cfg(feature = "media-preview")]
type PreviewSlot = Option<(i64, libtgt::model::MediaFile, u16)>;
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
            #[cfg(feature = "media-preview")]
            previews: Default::default(),
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
            MessageCommand::DownloadMedia if entry.media().is_some() => {
                self.send(Action::DownloadMedia(message_id));
            }
            MessageCommand::OpenMedia | MessageCommand::DownloadMedia => {
                self.send(Action::StatusMessage("This message has no media.".into()));
            }
            MessageCommand::React => self.send(Action::LoadReactions(message_id)),
            MessageCommand::OpenLink => match entry.links().as_slice() {
                [] => self.send(Action::StatusMessage("This message has no links.".into())),
                [link] => self.send(Action::OpenUrl(link.clone())),
                links => self.send(Action::ShowPicker(PickerKind::Links(links.to_vec()))),
            },
            MessageCommand::GoToReply => match entry.reply_to() {
                Some(TdMessageReplyTo::Message(reply))
                    if reply.chat_id == self.app_context.store().open_chat_id().as_i64() =>
                {
                    self.send(Action::JumpToMessage(reply.message_id));
                }
                _ => self.send(Action::StatusMessage(
                    "This message isn't a reply in this chat.".into(),
                )),
            },
            MessageCommand::TogglePin => {
                self.send(Action::SetPinned(message_id, !entry.is_pinned()));
            }
            MessageCommand::CopyLink => self.send(Action::CopyMessageLink(message_id)),
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
    /// The image shown under a media message in the chat.
    #[cfg(feature = "media-preview")]
    fn preview_file(entry: &MessageEntry) -> Option<&libtgt::model::MediaFile> {
        entry.media()?.inline_preview()
    }

    /// Rows reserved under a message for its preview.
    fn preview_rows(_entry: &MessageEntry) -> u16 {
        #[cfg(feature = "media-preview")]
        if Self::preview_file(_entry).is_some() {
            return crate::ui::preview::CHAT_PREVIEW_ROWS;
        }
        0
    }

    /// ` 👥 Name · status ` or ` 👥 Name › 💬 Topic `, like concord's channel
    /// pane title.
    fn pane_title(&self) -> Line<'static> {
        let store = self.app_context.store();
        let theme = self.app_context.theme();
        let Some(name) = store.name_of_open_chat_id() else {
            return Line::from(pane::title("Chat"));
        };
        let mut spans = vec![Span::raw(" ")];
        if let Some(kind) = store.open_chat_kind() {
            spans.push(Span::styled(
                format!("{} ", kind.icon()),
                theme.style(HighlightGroup::ChatTypeMarker),
            ));
        }
        spans.push(Span::raw(name));
        if let Some(topic) = store.open_topic() {
            spans.push(Span::styled(
                format!(" › {} ", topic.icon()),
                theme.style(HighlightGroup::ChatTypeMarker),
            ));
            spans.push(Span::raw(topic.name));
        }
        let status = store.open_chat_user_status();
        if !status.trim().is_empty() {
            spans.push(Span::styled(
                format!(" · {}", status.trim()),
                theme.style(HighlightGroup::Description),
            ));
        }
        spans.push(Span::raw(" "));
        Line::from(spans)
    }

    /// Draws the previews of the messages fully on screen, loading missing
    /// ones. `heights` and `slots` follow the list's items, newest first, as
    /// they're stacked up from the bottom of `inner`.
    #[cfg(feature = "media-preview")]
    fn draw_previews(
        &mut self,
        frame: &mut ratatui::Frame<'_>,
        inner: Rect,
        heights: &[u16],
        slots: &[PreviewSlot],
    ) {
        use crate::ui::preview::{CHAT_PREVIEW_COLUMNS, CHAT_PREVIEW_ROWS};

        let indent = message::MESSAGE_PREFIX_WIDTH;
        let width = CHAT_PREVIEW_COLUMNS.min(inner.width.saturating_sub(2 * indent));
        let placeholder_style = self.app_context.style(HighlightGroup::Placeholder);
        let mut bottom = inner.bottom();
        let offset = self.message_list_state.offset();
        for (height, slot) in heights.iter().zip(slots).skip(offset) {
            // Partly visible items are cut: skip their image
            if *height > bottom - inner.y {
                break;
            }
            let top = bottom - height;
            if let Some((message_id, file, row)) = slot {
                if let Some(action) = self.previews.request(*message_id, file) {
                    self.send(action);
                }
                let area = Rect::new(inner.x + indent, top + row, width, CHAT_PREVIEW_ROWS);
                self.previews
                    .draw(frame, file.id, area, Alignment::Left, placeholder_style);
            }
            bottom = top;
        }
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
            #[cfg(feature = "media-preview")]
            Action::PreviewDecoded(payload) => self.previews.decoded(payload),
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
        // One pane titled with the chat; the pinned strip (label and preview rows) on top
        let app_context = Arc::clone(&self.app_context);
        let theme = app_context.theme();
        // Padded on top like the gap under the newest message
        let block = pane::block(theme, self.pane_title(), self.focused).padding(Padding::top(1));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if app_context.store().open_chat_id().as_i64() == 0 {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "Select a chat to start messaging",
                    theme.style(HighlightGroup::Placeholder),
                )))
                .alignment(Alignment::Center),
                Rect::new(
                    inner.x,
                    inner.y + inner.height / 2,
                    inner.width,
                    1.min(inner.height),
                ),
            );
            return Ok(());
        }
        let pin_h = if has_pins { 2u16 } else { 0u16 };
        let chat_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(pin_h), Constraint::Min(1)])
            .split(inner);
        let list_area = chat_layout[1];
        let list_inner = list_area;
        let store = self.app_context.store();
        let reply_message_id = store.reply_message_id().as_i64();
        let me = store.me();
        let last_read_inbox = store.last_read_inbox_message_id();
        let first_unread = (last_read_inbox != 0)
            .then(|| {
                self.message_list
                    .iter()
                    .position(|m| m.sender_id() != me && m.id() > last_read_inbox)
            })
            .flatten();
        let selected = self.message_list_state.selected();
        let mut items: Vec<ListItem<'_>> = Vec::with_capacity(self.message_list.len() + 1);
        #[cfg(feature = "media-preview")]
        let mut slots: Vec<PreviewSlot> = Vec::with_capacity(self.message_list.len() + 1);
        for (index, entry) in self.message_list.iter().enumerate() {
            let previous = index
                .checked_sub(1)
                .and_then(|index| self.message_list.get(index));
            let next = self.message_list.get(index + 1);
            let row = MessageRow {
                show_header: message::starts_author_group(previous, entry),
                bottom_gap: next.is_none_or(|next| message::starts_author_group(Some(entry), next)),
                new_day: message::starts_new_day(previous, entry),
                first_unread: first_unread == Some(index),
                selected: selected == Some(index),
                reply_target: reply_message_id != 0 && entry.id() == reply_message_id,
                preview_rows: Self::preview_rows(entry),
            };
            let laid_out = message::message_lines(entry, row, &self.app_context, list_inner.width);
            #[cfg(feature = "media-preview")]
            slots.push(
                laid_out
                    .preview_row
                    .zip(Self::preview_file(entry))
                    .map(|(row, file)| (entry.id(), file.clone(), row)),
            );
            items.push(ListItem::new(laid_out.lines));
        }
        if self.app_context.store().is_history_loading() {
            items.push(ListItem::new(Line::from(Span::styled(
                "Loading…",
                self.app_context.style(HighlightGroup::Loading),
            ))));
        }

        #[cfg(feature = "media-preview")]
        let heights: Vec<u16> = {
            slots.resize(items.len(), None);
            slots.reverse();
            items
                .iter()
                .rev()
                .map(|item| item.height() as u16)
                .collect()
        };

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

        // The selected message is framed by a card instead of a highlight
        let list = List::new(items)
            .repeat_highlight_symbol(true)
            .direction(ListDirection::BottomToTop);

        if pin_h > 0 {
            let pin_inner = chat_layout[0];
            let pin_rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Length(1)])
                .split(pin_inner);

            let open_keys = self.pinned_messages_popup_keys_hint();
            let pin_title = Paragraph::new(Line::from(vec![
                Span::styled("📌 Pinned", theme.style(HighlightGroup::PinnedBadge)),
                Span::styled(
                    format!("  {open_keys} to browse"),
                    theme.style(HighlightGroup::Hint),
                ),
            ]));

            let pin_preview = self.pinned_preview_text();
            let pin_line = Line::from(Span::styled(
                if pin_preview.is_empty() {
                    "(no preview)".to_string()
                } else {
                    pin_preview
                },
                theme.style(HighlightGroup::ChatPreview),
            ));
            let pin_preview_para = Paragraph::new(pin_line);

            frame.render_widget(pin_title, pin_rows[0]);
            frame.render_widget(pin_preview_para, pin_rows[1]);
        }
        frame.render_stateful_widget(list, list_area, &mut self.message_list_state);
        #[cfg(feature = "media-preview")]
        self.draw_previews(frame, list_inner, &heights, &slots);

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
