use crate::app::action::Action;
use crate::app::context::AppContext;
use crate::settings::{HighlightGroup, Theme};
use crate::ui::chat_actions::chat_command_action;
use crate::ui::component_name::ComponentName;
use crate::ui::component_name::ComponentName::Prompt;
use crate::ui::component_traits::{Component, HandleFocus};
use crate::ui::{message, pane};
use crossterm::event::{KeyCode, MouseButton, MouseEventKind};
use libtgt::model::{ChatKind, ChatListEntry, MessageEntry, format_timestamp};
use libtgt::tdlib_rs::enums::{ChatList, UserStatus};
use nucleo_matcher::{Matcher, Utf32Str};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{List, ListDirection, ListState, Paragraph};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use unicode_width::UnicodeWidthStr;

/// `left`, then `right` aligned to the end of a `width` columns wide row.
pub(crate) fn spread(
    mut left: Vec<Span<'static>>,
    right: Vec<Span<'static>>,
    width: usize,
) -> Line<'static> {
    let used: usize = left.iter().chain(&right).map(Span::width).sum();
    left.push(Span::raw(" ".repeat(width.saturating_sub(used).max(1))));
    left.extend(right);
    Line::from(left)
}

/// Rows each chat takes in the list: its two lines and a blank spacer.
pub(crate) const CHAT_LIST_ITEM_ROWS: u16 = 3;

/// The chat list row, like concord's channel list: the selection marker, the
/// chat's icon and name with the time of its last message, then a preview of
/// that message with the badges (pinned, mentions, unread), and a blank line
/// that keeps it apart from the next chat.
pub(crate) fn chat_list_item_text(
    entry: &ChatListEntry,
    app_context: &AppContext,
    width: usize,
    selected: bool,
) -> Text<'static> {
    let theme = app_context.theme();
    let marker = pane::selection_marker(theme, selected);
    let icon = format!("{} ", entry.kind.icon());
    let indent = marker.width() + icon.width();
    let unread = entry.unread_count > 0 || entry.is_marked_as_unread;

    // Line 1: icon, name, online dot ... time
    let time: Vec<Span<'static>> = entry
        .last_message
        .as_ref()
        .map(|message| {
            vec![Span::styled(
                format_timestamp(message.timestamp()),
                theme.style(HighlightGroup::Timestamp),
            )]
        })
        .unwrap_or_default();
    let online = entry
        .user
        .as_ref()
        .is_some_and(|user| matches!(user.status, UserStatus::Online(_)));
    let online_width = if online { 2 } else { 0 };
    let time_width: usize = time.iter().map(Span::width).sum();
    let name_group = if entry.is_muted {
        HighlightGroup::MutedChat
    } else if unread {
        HighlightGroup::NavigationUnread
    } else {
        HighlightGroup::Normal
    };
    let name = message::truncate_display_width(
        &entry.chat_name,
        width
            .saturating_sub(indent + online_width + time_width + 1)
            .max(1),
    );
    let mut first = vec![
        marker,
        Span::styled(icon, theme.style(HighlightGroup::ChatTypeMarker)),
        Span::styled(name, theme.style(name_group)),
    ];
    if online {
        first.push(Span::styled(
            " ●",
            theme.style(HighlightGroup::PresenceOnline),
        ));
    }

    // Line 2: preview ... badges
    let mut badges = badge_spans(
        theme,
        entry.is_pinned,
        entry.unread_mention_count,
        entry.unread_count,
        entry.is_muted,
    );
    // Marked as unread without unread messages: an empty unread badge
    if entry.is_marked_as_unread && entry.unread_count == 0 {
        let group = if entry.is_muted {
            HighlightGroup::MutedChat
        } else {
            HighlightGroup::NotificationBadge
        };
        if !badges.is_empty() {
            badges.push(Span::raw(" "));
        }
        badges.push(Span::styled("  ", theme.style(group)));
    }
    let badges_width: usize = badges.iter().map(Span::width).sum();
    let preview_width = width.saturating_sub(indent + badges_width + 1).max(1);
    let mut second = vec![Span::raw(" ".repeat(indent))];
    match &entry.summary {
        Some(summary) => second.push(Span::styled(
            message::truncate_display_width(summary, preview_width),
            theme.style(HighlightGroup::ChatPreview),
        )),
        None => second.extend(preview_spans(
            entry.last_message.as_ref(),
            entry.kind.is_group(),
            app_context,
            preview_width,
        )),
    }

    let mut lines = vec![spread(first, time, width), spread(second, badges, width)];
    if selected {
        // Not the spacer, so the highlight stays on the chat itself
        for line in &mut lines {
            line.style = theme.style(HighlightGroup::SelectedRow);
        }
    }
    lines.push(Line::default());
    Text::from(lines)
}

/// The badges at the end of a row: pinned, mentions and the unread count,
/// a space apart.
pub(crate) fn badge_spans(
    theme: &Theme,
    is_pinned: bool,
    unread_mention_count: i32,
    unread_count: i32,
    is_muted: bool,
) -> Vec<Span<'static>> {
    let mut badges: Vec<Span<'static>> = Vec::new();
    if is_pinned {
        badges.push(Span::styled("📌", theme.style(HighlightGroup::PinnedBadge)));
    }
    if unread_mention_count > 0 {
        badges.push(Span::styled("@", theme.style(HighlightGroup::MentionBadge)));
    }
    if unread_count > 0 {
        let group = if is_muted {
            HighlightGroup::MutedChat
        } else {
            HighlightGroup::NotificationBadge
        };
        badges.push(Span::styled(
            format!(" {unread_count} "),
            theme.style(group),
        ));
    }
    badges
        .into_iter()
        .enumerate()
        .flat_map(|(index, badge)| {
            let gap = (index > 0).then(|| Span::raw(" "));
            gap.into_iter().chain(std::iter::once(badge))
        })
        .collect()
}

/// `You: text`, `Name: text` in groups, or the text of the last message,
/// on one line of `width` columns.
pub(crate) fn preview_spans(
    last_message: Option<&MessageEntry>,
    is_group: bool,
    app_context: &AppContext,
    width: usize,
) -> Vec<Span<'static>> {
    let theme = app_context.theme();
    let text = last_message
        .map(|message| message.message_content_to_string())
        .unwrap_or_default();
    let first_line = text.lines().map(str::trim).find(|line| !line.is_empty());
    let (Some(message), Some(first_line)) = (last_message, first_line) else {
        return vec![Span::styled(
            "No messages",
            theme.style(HighlightGroup::Placeholder),
        )];
    };
    let store = app_context.store();
    let author = if message.sender_id() == store.me() {
        Some("You".to_owned())
    } else if is_group {
        store.try_name_from_chats_or_users(message.sender_id())
    } else {
        None
    };
    let mut spans = Vec::new();
    let mut left = width;
    if let Some(author) = author.filter(|author| !author.is_empty()) {
        let author = message::truncate_display_width(&format!("{author}: "), left / 2);
        left = left.saturating_sub(author.width());
        spans.push(Span::styled(author, theme.style(HighlightGroup::Muted)));
    }
    spans.push(Span::styled(
        message::truncate_display_width(first_line, left.max(1)),
        theme.style(HighlightGroup::ChatPreview),
    ));
    spans
}

/// Opens `chat`, from the chat list or the archived chats: its messages are
/// loaded and the prompt focused, or for a forum the topic list. An already
/// open chat is kept as it is.
pub(crate) fn open_chat(app_context: &AppContext, chat: &ChatListEntry) {
    let store = app_context.store();
    let action_tx = app_context.action_tx();
    let open_id = store.open_chat_id().as_i64();
    tracing::info!(
        open_id,
        selected_chat_id = chat.chat_id,
        "open_chat: comparing open vs selected chat"
    );
    let is_forum = store.is_forum(chat.chat_id);
    // A forum's messages are in its topics: pick one first
    let next_pane = if is_forum {
        ComponentName::TopicList
    } else {
        Prompt
    };
    if open_id == chat.chat_id {
        // Same chat already open: don't clear/reload (avoids wiping messages on re-confirm).
        let _ = action_tx.send(Action::FocusComponent(next_pane));
        let _ = action_tx.send(Action::LoadPinnedMessages);
        let _ = action_tx.send(Action::ViewAllMessages);
        return;
    }
    store.set_open_chat_user(chat.user.clone());
    store.set_open_chat_id_i64(chat.chat_id);
    store.set_open_chat_is_forum(is_forum);
    store.set_open_topic_id(None);
    store.set_open_chat_topics(Vec::new());
    store.clear_open_chat_messages();
    store.set_open_chat_pinned(Vec::new());
    store.set_jump_target_message_id_i64(0);
    if is_forum {
        let _ = action_tx.send(Action::LoadForumTopics);
        let _ = action_tx.send(Action::FocusComponent(next_pane));
        return;
    }
    let _ = action_tx.send(Action::FocusComponent(Prompt));
    // Start loading chat history immediately (same handle_app_actions run).
    let _ = action_tx.send(Action::GetChatHistory);
    let _ = action_tx.send(Action::LoadPinnedMessages);
    // Mark all unread messages as read
    let _ = action_tx.send(Action::ViewAllMessages);
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
    /// Where the rows were drawn last (for click-to-select item when focused).
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
    /// Confirm the selection of the chat item in the list: the archive row
    /// shows the archived chats, any other opens its chat.
    fn confirm_selection(&mut self) {
        let Some(chat) = self
            .chat_list_state
            .selected()
            .and_then(|i| self.visible_chats.get(i))
        else {
            return;
        };
        let action_tx = self.app_context.action_tx();
        if chat.kind == ChatKind::Archive {
            let _ = action_tx.send(Action::FocusComponent(ComponentName::ArchiveList));
            return;
        }
        // Only open a different chat when ChatList has focus (avoids clearing
        // if ChatListOpen was forwarded while user had Chat or Prompt focused).
        let open_id = self.app_context.store().open_chat_id().as_i64();
        if open_id != chat.chat_id
            && self.app_context.focused_component() != Some(ComponentName::ChatList)
        {
            return;
        }
        // Left the archive for a chat outside it
        let _ = action_tx.send(Action::CloseArchiveList);
        open_chat(&self.app_context, chat);
    }

    /// Runs `action` (the chat actions menu or one of its commands) on the
    /// selected chat.
    fn act_on_selected(&self, action: &Action) {
        let action_tx = self.app_context.action_tx();
        let chat = self
            .chat_list_state
            .selected()
            .and_then(|i| self.visible_chats.get(i));
        let chat_id = match chat {
            Some(chat) if chat.kind != ChatKind::Archive => chat.chat_id,
            Some(_) => {
                let _ = action_tx.send(Action::StatusMessage(
                    "Open the archive to act on its chats.".into(),
                ));
                return;
            }
            None => return,
        };
        let _ = action_tx.send(match action {
            Action::SelectedChat(command) => chat_command_action(*command, chat_id),
            _ => Action::ShowChatActionsFor(chat_id, None),
        });
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
        // The archive row has no chat id
        let row = self
            .visible_chats
            .iter()
            .position(|c| c.kind != ChatKind::Archive && c.chat_id == open_id);
        match row {
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
        let store = self.app_context.store();
        // The archive sits on top of the chats, like in Telegram
        let mut items: Vec<ChatListEntry> = store.archive_entry().into_iter().collect();
        items.extend(store.chat_list());
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
        self.app_context.set_typing(true);
        self.search_input.clear();
    }

    /// Exit search mode.
    fn stop_search(&mut self) {
        self.search_mode = false;
        self.app_context.set_typing(false);
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
                // Use actual first visible item index from last draw (not inferred from selected).
                let offset = self.last_list_offset.unwrap_or(0);
                let item_index = pane::item_at(
                    list_area,
                    mouse.column,
                    mouse.row,
                    offset,
                    CHAT_LIST_ITEM_ROWS,
                );
                if let Some(item_index) = item_index
                    && item_index < self.visible_chats.len()
                {
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
            Action::ShowChatActions | Action::SelectedChat(_) => self.act_on_selected(&action),
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
        let app_context = Arc::clone(&self.app_context);
        let theme = app_context.theme();

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
        // Lazy init: populate visible_chats on first draw when data is available (initial LoadChats
        // may run before TDLib has delivered chat list updates).
        if self.visible_chats.is_empty() {
            self.rebuild_visible_chats();
        }

        // Draw only from visible_chats (filter/sort done in update() via rebuild_visible_chats).
        // Draw search bar if in search mode
        if let Some(search_rect) = search_area {
            let search_block = pane::block(theme, pane::title("Search chats"), self.focused);
            let search_text = format!("{}_", self.search_input);
            let search_paragraph = Paragraph::new(search_text).block(search_block);
            frame.render_widget(search_paragraph, search_rect);

            // Set cursor position in search bar
            if self.focused && self.search_mode {
                frame.set_cursor_position(Position {
                    x: search_rect.x + self.search_input.len() as u16 + 1,
                    y: search_rect.y + 1,
                });
            }
        }

        let block = pane::list_block(theme, pane::title("Chats"), self.focused);
        let inner = block.inner(list_area);
        self.last_list_area = Some(inner);
        let width = usize::from(inner.width);
        let selected = self.chat_list_state.selected();
        let items = self.visible_chats.iter().enumerate().map(|(index, item)| {
            chat_list_item_text(item, &self.app_context, width, selected == Some(index))
        });
        // The selection is drawn by the rows: marker and highlighted background
        let list = List::new(items)
            .block(block)
            .direction(ListDirection::TopToBottom);

        frame.render_stateful_widget(list, list_area, &mut self.chat_list_state);
        // Store first visible item index for click-to-item (List updates state.offset on render).
        self.last_list_offset = Some(self.chat_list_state.offset());
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/chat_list_window.rs"]
mod tests;
