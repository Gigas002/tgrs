use crate::app::action::Action;
use crate::app::context::AppContext;
use crate::ui::component_name::ComponentName;
use crate::ui::component_name::ComponentName::Prompt;
use crate::ui::component_traits::{Component, HandleFocus};
use crate::ui::message;
use crossterm::event::{KeyCode, MouseButton, MouseEventKind};
use libtgt::model::ChatListEntry;
use libtgt::tdlib_rs::enums::{ChatList, UserStatus};
use nucleo_matcher::{Matcher, Utf32Str};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::symbols::border::PLAIN;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, List, ListDirection, ListState, Paragraph};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

/// The two-line chat list row: name, unread count, time, then a message preview.
pub(crate) fn chat_list_item_text(
    entry: &ChatListEntry,
    app_context: &AppContext,
) -> Text<'static> {
    let pin_symbol = if entry.is_pinned { "📌 " } else { "" };
    let mut online_symbol = "";
    let mut verificated_symbol = "";
    if let Some(user) = &entry.user {
        online_symbol = match user.status {
            UserStatus::Online(_) => "🟢 ",
            UserStatus::Offline(_) => "",
            UserStatus::Empty => "",
            UserStatus::Recently(_) => "",
            UserStatus::LastWeek(_) => "",
            UserStatus::LastMonth(_) => "",
        };
        // tdlib-rs 1.3+: User.is_verified removed; verified badge disabled until we map from new API
        verificated_symbol = "";
    }
    let unread_info = if entry.is_marked_as_unread {
        format!("({})", entry.unread_count)
    } else {
        "".to_string()
    };

    let preview_lines = -1;
    let mut text = Text::default();
    text.extend(vec![Line::from(vec![
        Span::raw(pin_symbol),
        Span::raw(online_symbol),
        Span::styled(
            entry.chat_name.clone(),
            app_context.style_chat_list_item_chat_name(),
        ),
        Span::raw(" "),
        Span::styled(
            unread_info,
            app_context.style_chat_list_item_unread_counter(),
        ),
        Span::raw(" "),
        Span::raw(verificated_symbol),
        Span::raw(" | "),
        entry.last_message.as_ref().map_or_else(Span::default, |e| {
            message::timestamp_span(e.timestamp(), app_context)
        }),
    ])]);
    // Always add a second line (message preview or "[No message]") so every item is 2 rows for consistent height and click mapping.
    // Treat missing last_message or empty content (e.g. "joined Telegram" → empty message from backend) as no message.
    let second_line = match &entry.last_message {
        None => Line::from(Span::styled(
            "[No message]",
            app_context.style_chat_list_item_message_content(),
        )),
        Some(e) if e.message_content_to_string().trim().is_empty() => Line::from(Span::styled(
            "[No message]",
            app_context.style_chat_list_item_message_content(),
        )),
        Some(e) => message::content_lines(
            e,
            app_context.style_chat_list_item_message_content(),
            preview_lines,
        )
        .into_iter()
        .next()
        .unwrap_or_else(|| {
            Line::from(Span::styled(
                "[No message]",
                app_context.style_chat_list_item_message_content(),
            ))
        }),
    };
    text.extend(second_line);

    text
}
/// `ChatListWindow` is a struct that represents a window for displaying a list
/// of chat items. It is responsible for managing the layout and rendering of
/// the chat list.
pub struct ChatListWindow {
    /// The application context.
    app_context: Arc<AppContext>,
    /// The name of the `ChatListWindow`.
    name: String,
    /// An unbounded sender that send action for processing.
    command_tx: Option<UnboundedSender<Action>>,
    /// Visible chat list (filtered/sorted). Updated in update(); draw() only renders this.
    visible_chats: Vec<ChatListEntry>,
    /// The state of the list.
    chat_list_state: ListState,
    /// Indicates whether the `ChatListWindow` is focused or not.
    focused: bool,
    /// String used to sort chats.
    sort_string: Option<String>,
    /// Search input string for filtering chats.
    search_input: String,
    /// Whether search mode is active.
    search_mode: bool,
    /// Last list content area (for click-to-select item when focused).
    last_list_area: Option<Rect>,
    /// First visible item index after last draw (from ListState offset; used for click-to-item).
    last_list_offset: Option<usize>,
}
/// Implementation of the `ChatListWindow` struct.
impl ChatListWindow {
    /// Create a new instance of the `ChatListWindow` struct.
    ///
    /// # Arguments
    /// * `app_context` - An Arc wrapped AppContext struct.
    ///
    /// # Returns
    /// * `Self` - The new instance of the `ChatListWindow` struct.
    pub fn new(app_context: Arc<AppContext>) -> Self {
        let name = "".to_string();
        let command_tx = None;
        let visible_chats = vec![];
        let chat_list_state = ListState::default();
        let focused = false;
        let sort_string = None;

        ChatListWindow {
            app_context,
            name,
            command_tx,
            visible_chats,
            chat_list_state,
            focused,
            sort_string,
            search_input: String::new(),
            search_mode: false,
            last_list_area: None,
            last_list_offset: None,
        }
    }
    /// Set the name of the `ChatListWindow`.
    ///
    /// # Arguments
    /// * `name` - The name of the `ChatListWindow`.
    ///
    /// # Returns
    /// * `Self` - The modified instance of the `ChatListWindow`.
    pub fn with_name(mut self, name: impl AsRef<str>) -> Self {
        self.name = name.as_ref().to_string();
        self
    }
    /// Select the next chat item in the list.
    fn next(&mut self) {
        let i = match self.chat_list_state.selected() {
            Some(i) => {
                if i == self.visible_chats.len() / 2 {
                    let _ = self
                        .app_context
                        .action_tx()
                        .send(Action::LoadChats(ChatList::Main.into(), 20));
                }

                if i >= self.visible_chats.len() - 1 {
                    i
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.chat_list_state.select(Some(i));
    }
    /// Select the previous chat item in the list.
    fn previous(&mut self) {
        let i = match self.chat_list_state.selected() {
            Some(i) => {
                if i == 0 {
                    0
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.chat_list_state.select(Some(i));
    }
    /// Unselect the chat item in the list.
    fn unselect(&mut self) {
        self.chat_list_state.select(None);
    }
    /// Confirm the selection of the chat item in the list.
    fn confirm_selection(&mut self) {
        if let Some(i) = self.chat_list_state.selected()
            && let Some(chat) = self.visible_chats.get(i)
        {
            // Explicit i64 comparison (TDLib uses int64); avoid any type/guard mismatch.
            let open_id: i64 = self.app_context.store().open_chat_id().as_i64();
            let selected_chat_id: i64 = chat.chat_id;
            tracing::info!(
                open_id,
                selected_chat_id,
                selected_index = i,
                "confirm_selection: comparing open vs selected chat"
            );
            if open_id == selected_chat_id {
                // Same chat already open: don't clear/reload (avoids wiping messages on re-confirm).
                self.app_context
                    .action_tx()
                    .send(Action::FocusComponent(Prompt))
                    .unwrap();
                let _ = self
                    .app_context
                    .action_tx()
                    .send(Action::LoadPinnedMessages);
                let _ = self.app_context.action_tx().send(Action::ViewAllMessages);
                return;
            }
            // Only clear and open a different chat when ChatList has focus (avoids clearing
            // if ChatListOpen was forwarded while user had Chat or Prompt focused).
            if self.app_context.focused_component() != Some(ComponentName::ChatList) {
                return;
            }
            self.app_context
                .store()
                .set_open_chat_user(chat.user.clone());
            self.app_context.store().set_open_chat_id_i64(chat.chat_id);
            self.app_context.store().clear_open_chat_messages();
            self.app_context.store().set_open_chat_pinned(Vec::new());
            self.app_context.store().set_jump_target_message_id_i64(0);
            self.app_context
                .action_tx()
                .send(Action::FocusComponent(Prompt))
                .unwrap();

            // Start loading chat history immediately (same handle_app_actions run).
            let _ = self.app_context.action_tx().send(Action::GetChatHistory);
            let _ = self
                .app_context
                .action_tx()
                .send(Action::LoadPinnedMessages);

            // Mark all unread messages as read
            let _ = self.app_context.action_tx().send(Action::ViewAllMessages);
        }
    }

    /// Sets the string used to order the entries of the chat list.
    fn sort(&mut self, s: String) {
        self.sort_string = Some(s);
    }

    /// Unsets the string used to order the entries of the chat list.
    fn default_sort(&mut self) {
        self.sort_string = None;
    }

    /// Sync list selection to the currently open chat (by chat_id). Uses visible_chats so logic and UI match.
    /// If open_chat_id is not in visible_chats (e.g. during TDLib update flurry), clear selection
    /// so we don't confirm a wrong chat at a stale index.
    fn sync_selection_to_open_chat(&mut self) {
        let open_id: i64 = self.app_context.store().open_chat_id().as_i64();
        match self.visible_chats.iter().position(|c| c.chat_id == open_id) {
            Some(idx) => self.chat_list_state.select(Some(idx)),
            None => {
                tracing::info!(
                    open_id,
                    visible_len = self.visible_chats.len(),
                    "sync_selection_to_open_chat: open_chat_id not in visible_chats, clearing selection"
                );
                self.chat_list_state.select(None);
            }
        }
    }

    /// Rebuild visible_chats from get_chats_index() with current filter/sort; then sync selection.
    /// Call from update() so draw() and confirm_selection() see the same list (no race with draw() re-sort).
    ///
    /// # Arguments
    /// * `preserve_selection` - If true, preserve the current selection index (clamp if needed) instead of syncing to open chat.
    fn rebuild_visible_chats(&mut self) {
        self.rebuild_visible_chats_with_preserve_selection(false);
    }

    /// Rebuild visible_chats with option to preserve selection.
    fn rebuild_visible_chats_with_preserve_selection(&mut self, preserve_selection: bool) {
        let current_selection = self.chat_list_state.selected();
        let mut items = self.app_context.store().chat_list();
        if let Some(s) = self.sort_string.as_ref()
            && !s.is_empty()
        {
            let mut config = nucleo_matcher::Config::DEFAULT;
            config.prefer_prefix = true;
            let mut matcher = Matcher::new(config.clone());
            let search_chars: Vec<char> = s.chars().collect();
            items.retain(|chat| {
                let chat_name_chars: Vec<char> = chat.chat_name.chars().collect();
                matcher
                    .fuzzy_indices(
                        Utf32Str::Unicode(&chat_name_chars),
                        Utf32Str::Unicode(&search_chars),
                        &mut Vec::new(),
                    )
                    .is_some()
            });
            let mut matcher = Matcher::new(config);
            items.sort_by(|a, b| {
                let a: Vec<char> = a.chat_name.chars().collect();
                let b: Vec<char> = b.chat_name.chars().collect();
                let a_score = matcher
                    .fuzzy_indices(
                        Utf32Str::Unicode(&a),
                        Utf32Str::Unicode(&search_chars),
                        &mut Vec::new(),
                    )
                    .unwrap_or(0);
                let b_score = matcher
                    .fuzzy_indices(
                        Utf32Str::Unicode(&b),
                        Utf32Str::Unicode(&search_chars),
                        &mut Vec::new(),
                    )
                    .unwrap_or(0);
                a_score.cmp(&b_score)
            });
            items.reverse();
        }
        self.visible_chats = items;

        if preserve_selection {
            // Preserve current selection index, clamping if needed
            if let Some(idx) = current_selection {
                let clamped_idx = idx.min(self.visible_chats.len().saturating_sub(1));
                self.chat_list_state
                    .select(if self.visible_chats.is_empty() {
                        None
                    } else {
                        Some(clamped_idx)
                    });
            }
        } else {
            // Sync selection to currently open chat
            self.sync_selection_to_open_chat();
        }
    }

    /// Enter search mode.
    fn start_search(&mut self) {
        self.search_mode = true;
        self.search_input.clear();
    }

    /// Exit search mode.
    fn stop_search(&mut self) {
        self.search_mode = false;
        self.search_input.clear();
        self.default_sort();
    }

    /// Handle character input in search mode.
    fn handle_search_char(&mut self, c: char) {
        self.search_input.push(c);
        self.sort(self.search_input.clone());
        self.rebuild_visible_chats();
    }

    /// Handle backspace in search mode.
    fn handle_search_backspace(&mut self) {
        self.search_input.pop();
        if self.search_input.is_empty() {
            self.default_sort();
        } else {
            self.sort(self.search_input.clone());
        }
        self.rebuild_visible_chats();
    }
}

/// Implement the `HandleFocus` trait for the `ChatListWindow` struct.
/// This trait allows the `ChatListWindow` to be focused or unfocused.
impl HandleFocus for ChatListWindow {
    /// Set the `focused` flag for the `ChatListWindow`.
    fn focus(&mut self) {
        self.focused = true;
    }
    /// Set the `focused` flag for the `ChatListWindow`.
    fn unfocus(&mut self) {
        self.focused = false;
    }
}

/// Implement the `Component` trait for the `ChatListWindow` struct.
impl Component for ChatListWindow {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> std::io::Result<()> {
        self.command_tx = Some(tx.clone());
        Ok(())
    }

    fn handle_mouse_events(
        &mut self,
        mouse: crossterm::event::MouseEvent,
    ) -> std::io::Result<Option<Action>> {
        match mouse.kind {
            MouseEventKind::ScrollDown if self.focused => Ok(Some(Action::ChatListNext)),
            MouseEventKind::ScrollUp if self.focused => Ok(Some(Action::ChatListPrevious)),
            MouseEventKind::Down(MouseButton::Left) => {
                // When focused: single click on item opens that chat. When not focused: return None
                // (CoreWindow will focus chat list on first click; second click then opens).
                if !self.focused {
                    return Ok(None);
                }
                let Some(list_area) = self.last_list_area else {
                    return Ok(None);
                };
                let col = mouse.column;
                let row = mouse.row;
                let in_rect = col >= list_area.x
                    && col < list_area.x + list_area.width
                    && row >= list_area.y
                    && row < list_area.y + list_area.height;
                if !in_rect || self.visible_chats.is_empty() {
                    return Ok(None);
                }
                // List content: block has top border (1 row); items start at area.y+1.
                // Each chat list item is 2 rows (name line + last message line).
                const ROWS_PER_ITEM: u16 = 2;
                let content_y = list_area.y + 1; // below top border
                let content_height = list_area.height.saturating_sub(2); // top and bottom border
                let row_in_content = row.saturating_sub(content_y);
                if row_in_content >= content_height {
                    return Ok(None);
                }
                // Use actual first visible item index from last draw (not inferred from selected).
                let offset = self.last_list_offset.unwrap_or(0);
                let item_index = offset + (row_in_content as usize / ROWS_PER_ITEM as usize);
                if item_index < self.visible_chats.len() {
                    self.chat_list_state.select(Some(item_index));
                    return Ok(Some(Action::ChatListOpen));
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    fn update(&mut self, action: Action) {
        match action {
            Action::ChatListNext => {
                // Allow navigation in and out of search mode (arrow keys move list selection)
                self.next();
            }
            Action::ChatListPrevious => {
                // Allow navigation in and out of search mode (arrow keys move list selection)
                self.previous();
            }
            Action::ChatListUnselect => {
                if self.search_mode {
                    self.stop_search();
                } else {
                    self.unselect();
                }
            }
            Action::ChatListOpen => {
                // Open selected chat; in search mode also exit search after opening
                self.confirm_selection();
                if self.search_mode {
                    self.stop_search();
                }
            }
            Action::ChatListSortWithString(s) => {
                self.sort(s);
                self.rebuild_visible_chats();
            }
            Action::ChatListRestoreSort => {
                self.default_sort();
                self.rebuild_visible_chats();
            }
            Action::ChatListSearch => self.start_search(),
            Action::LoadChats(..) => {
                // Preserve selection when loading more chats (user is scrolling)
                self.rebuild_visible_chats_with_preserve_selection(true);
            }
            Action::ChatHistoryAppended | Action::Resize(..) => {
                // During startup, TDLib can emit a large burst of chat updates (positions, last_message, unread, etc.).
                // Rebuilding the list is necessary, but re-syncing selection to the open chat on every burst makes the
                // chat list feel "stuck" (user can't navigate because selection gets overwritten).
                //
                // Preserve selection while the chat list is focused; otherwise keep syncing selection to open chat.
                self.rebuild_visible_chats_with_preserve_selection(self.focused);
            }
            Action::FocusComponent(ComponentName::ChatList) => {
                self.rebuild_visible_chats();
            }
            Action::Key(key_code, modifiers) if self.search_mode => {
                match key_code {
                    KeyCode::Char(c)
                        if !modifiers.control && !modifiers.alt && !modifiers.shift =>
                    {
                        self.handle_search_char(c);
                    }
                    KeyCode::Backspace => {
                        self.handle_search_backspace();
                    }
                    KeyCode::Enter => {
                        // Open selected chat and exit search mode
                        self.confirm_selection();
                        self.stop_search();
                    }
                    KeyCode::Esc => {
                        self.stop_search();
                    }
                    KeyCode::Down => {
                        // Allow arrow key navigation in search mode
                        self.next();
                    }
                    KeyCode::Up => {
                        // Allow arrow key navigation in search mode
                        self.previous();
                    }
                    KeyCode::Tab => {
                        // Allow tab navigation in search mode
                        self.next();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect) -> std::io::Result<()> {
        let style_border_focused = if self.focused {
            self.app_context.style_border_component_focused()
        } else {
            self.app_context.style_chat_list()
        };

        // Split area to include search bar if in search mode
        let (list_area, search_area) = if self.search_mode {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Fill(1)])
                .split(area);
            (layout[1], Some(layout[0]))
        } else {
            (area, None)
        };
        self.last_list_area = Some(list_area);

        // Lazy init: populate visible_chats on first draw when data is available (initial LoadChats
        // may run before TDLib has delivered chat list updates).
        if self.visible_chats.is_empty() {
            self.rebuild_visible_chats();
        }

        // Draw only from visible_chats (filter/sort done in update() via rebuild_visible_chats).
        // Draw search bar if in search mode
        if let Some(search_rect) = search_area {
            let search_block = Block::default()
                .border_set(PLAIN)
                .border_style(style_border_focused)
                .borders(Borders::ALL)
                .title("Search chats");
            let search_text = format!("{}_", self.search_input);
            let search_paragraph = Paragraph::new(search_text)
                .block(search_block)
                .style(self.app_context.style_chat_list());
            frame.render_widget(search_paragraph, search_rect);

            // Set cursor position in search bar
            if self.focused && self.search_mode {
                frame.set_cursor_position(Position {
                    x: search_rect.x + self.search_input.len() as u16 + 1,
                    y: search_rect.y + 1,
                });
            }
        }

        let items = self
            .visible_chats
            .iter()
            .map(|item| chat_list_item_text(item, &self.app_context));
        let block = Block::default()
            .border_set(PLAIN)
            .border_style(style_border_focused)
            .borders(Borders::TOP | Borders::LEFT | Borders::BOTTOM)
            .title(Line::from(self.name.as_str()));

        let list = List::new(items)
            .block(block)
            .style(self.app_context.style_chat_list())
            .highlight_style(self.app_context.style_chat_list_item_selected())
            .direction(ListDirection::TopToBottom);
        // .highlight_symbol("➤ ")
        // .repeat_highlight_symbol(true)

        frame.render_stateful_widget(list, list_area, &mut self.chat_list_state);
        // Store first visible item index for click-to-item (List updates state.offset on render).
        self.last_list_offset = Some(self.chat_list_state.offset());
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/chat_list_window.rs"]
mod tests;
