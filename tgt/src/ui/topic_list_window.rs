//! The topics of the open forum chat, in a pane right of the chat list.

use crate::app::action::Action;
use crate::app::context::AppContext;
use crate::settings::HighlightGroup;
use crate::ui::chat_list_window::{
    CHAT_LIST_ITEM_ROWS, badge_spans, draft_spans, preview_spans, spread,
};
use crate::ui::component_name::ComponentName;
use crate::ui::component_traits::{Component, HandleFocus};
use crate::ui::{message, pane};
use crossterm::event::{MouseButton, MouseEventKind};
use libtgt::model::{TopicEntry, format_timestamp};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{List, ListDirection, ListState, Paragraph};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use unicode_width::UnicodeWidthStr;

/// The topic list row, like a chat list row: the selection marker, the
/// topic's icon and name with the time of its last message, then a preview of
/// that message with the badges, and a blank spacer.
pub(crate) fn topic_list_item_text(
    topic: &TopicEntry,
    app_context: &AppContext,
    width: usize,
    selected: bool,
    open: bool,
) -> Text<'static> {
    let theme = app_context.theme();
    let marker = pane::selection_marker(theme, selected);
    let icon = format!("{} ", topic.icon());
    let indent = marker.width() + icon.width();

    // Line 1: icon, name, lock ... time
    let time: Vec<Span<'static>> = topic
        .last_message
        .as_ref()
        .map(|message| {
            vec![Span::styled(
                format_timestamp(message.timestamp()),
                theme.style(HighlightGroup::Timestamp),
            )]
        })
        .unwrap_or_default();
    let lock_width = if topic.is_closed { 3 } else { 0 };
    let time_width: usize = time.iter().map(Span::width).sum();
    let name_group = if topic.is_muted {
        HighlightGroup::MutedChat
    } else if topic.unread_count > 0 {
        HighlightGroup::NavigationUnread
    } else if open {
        HighlightGroup::Strong
    } else {
        HighlightGroup::Normal
    };
    let name = message::truncate_display_width(
        &topic.name,
        width
            .saturating_sub(indent + lock_width + time_width + 1)
            .max(1),
    );
    let mut first = vec![
        marker,
        Span::styled(icon, theme.style(HighlightGroup::ChatTypeMarker)),
        Span::styled(name, theme.style(name_group)),
    ];
    if topic.is_closed {
        first.push(Span::styled(" 🔒", theme.style(HighlightGroup::Muted)));
    }

    // Line 2: preview ... badges
    let badges = badge_spans(
        theme,
        topic.is_pinned,
        topic.unread_mention_count,
        topic.unread_count,
        topic.is_muted,
    );
    let badges_width: usize = badges.iter().map(Span::width).sum();
    let preview_width = width.saturating_sub(indent + badges_width + 1).max(1);
    let mut second = vec![Span::raw(" ".repeat(indent))];
    // An unsent message shows instead of the last one, except in the open
    // topic, where it's in the prompt
    if !open && !topic.draft.trim().is_empty() {
        second.extend(draft_spans(theme, &topic.draft, preview_width));
    } else {
        second.extend(preview_spans(
            topic.last_message.as_ref(),
            true,
            app_context,
            preview_width,
        ));
    }

    let mut lines = vec![spread(first, time, width), spread(second, badges, width)];
    if selected {
        // Not the spacer, so the highlight stays on the topic itself
        for line in &mut lines {
            line.style = theme.style(HighlightGroup::SelectedRow);
        }
    }
    lines.push(Line::default());
    Text::from(lines)
}

/// `TopicListWindow` lists the topics of the open forum chat; opening one
/// shows its messages in the chat.
pub struct TopicListWindow {
    /// The application context.
    app_context: Arc<AppContext>,
    /// The name of the `TopicListWindow`.
    name: String,
    /// An unbounded sender that send action for processing.
    command_tx: Option<UnboundedSender<Action>>,
    /// The topics shown, refreshed from the store before each use.
    visible_topics: Vec<TopicEntry>,
    /// The selected topic, kept by id while topics move around.
    selected_topic_id: Option<i32>,
    /// The state of the list.
    topic_list_state: ListState,
    /// Indicates whether the `TopicListWindow` is focused or not.
    focused: bool,
    /// Where the rows were drawn last (for click-to-select item when focused).
    last_list_area: Option<Rect>,
    /// First visible item index after last draw (used for click-to-item).
    last_list_offset: Option<usize>,
}

impl TopicListWindow {
    /// Create a new instance of the `TopicListWindow` struct.
    pub fn new(app_context: Arc<AppContext>) -> Self {
        TopicListWindow {
            app_context,
            name: String::new(),
            command_tx: None,
            visible_topics: Vec::new(),
            selected_topic_id: None,
            topic_list_state: ListState::default(),
            focused: false,
            last_list_area: None,
            last_list_offset: None,
        }
    }

    /// Set the name of the `TopicListWindow`.
    pub fn with_name(mut self, name: impl AsRef<str>) -> Self {
        self.name = name.as_ref().to_string();
        self
    }

    /// Reloads the topics from the store. The selection stays on its topic,
    /// or goes to the open one, or the first.
    fn refresh(&mut self) {
        let store = self.app_context.store();
        self.visible_topics = store.open_chat_topics_snapshot();
        let index = [self.selected_topic_id, store.open_topic_id()]
            .into_iter()
            .flatten()
            .find_map(|id| self.visible_topics.iter().position(|t| t.topic_id == id))
            .or((!self.visible_topics.is_empty()).then_some(0));
        self.select(index);
    }

    fn select(&mut self, index: Option<usize>) {
        self.topic_list_state.select(index);
        self.selected_topic_id = index
            .and_then(|index| self.visible_topics.get(index))
            .map(|topic| topic.topic_id);
    }

    /// Moves the selection `step` topics, stopping at the ends.
    fn step(&mut self, step: isize) {
        self.refresh();
        if self.visible_topics.is_empty() {
            return;
        }
        let last = self.visible_topics.len() - 1;
        let index = self
            .topic_list_state
            .selected()
            .map_or(0, |index| index.saturating_add_signed(step).min(last));
        self.select(Some(index));
    }

    /// Opens the selected topic: its messages replace the chat's.
    fn confirm_selection(&mut self) {
        self.refresh();
        let Some(topic) = self
            .topic_list_state
            .selected()
            .and_then(|index| self.visible_topics.get(index))
        else {
            return;
        };
        let store = self.app_context.store();
        let action_tx = self.app_context.action_tx();
        if store.open_topic_id() != Some(topic.topic_id) {
            store.set_open_topic_id(Some(topic.topic_id));
            store.clear_open_chat_messages();
            store.set_open_chat_pinned(Vec::new());
            store.set_jump_target_message_id_i64(0);
            store.clear_reply_message();
            let _ = action_tx.send(Action::HideChatWindowReply);
            let _ = action_tx.send(Action::GetChatHistory);
        }
        let _ = action_tx.send(Action::FocusComponent(ComponentName::Prompt));
        let _ = action_tx.send(Action::LoadPinnedMessages);
        let _ = action_tx.send(Action::ViewAllMessages);
    }
}

impl HandleFocus for TopicListWindow {
    fn focus(&mut self) {
        self.focused = true;
    }
    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl Component for TopicListWindow {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> std::io::Result<()> {
        self.command_tx = Some(tx);
        Ok(())
    }

    fn handle_mouse_events(
        &mut self,
        mouse: crossterm::event::MouseEvent,
    ) -> std::io::Result<Option<Action>> {
        match mouse.kind {
            MouseEventKind::ScrollDown if self.focused => Ok(Some(Action::TopicListNext)),
            MouseEventKind::ScrollUp if self.focused => Ok(Some(Action::TopicListPrevious)),
            // The first click focuses the pane (CoreWindow), the next opens a topic
            MouseEventKind::Down(MouseButton::Left) if self.focused => {
                let Some(area) = self.last_list_area else {
                    return Ok(None);
                };
                let offset = self.last_list_offset.unwrap_or(0);
                let index =
                    pane::item_at(area, mouse.column, mouse.row, offset, CHAT_LIST_ITEM_ROWS);
                match index {
                    Some(index) if index < self.visible_topics.len() => {
                        self.select(Some(index));
                        Ok(Some(Action::TopicListOpen))
                    }
                    _ => Ok(None),
                }
            }
            _ => Ok(None),
        }
    }

    fn update(&mut self, action: Action) {
        match action {
            Action::TopicListNext => self.step(1),
            Action::TopicListPrevious => self.step(-1),
            Action::TopicListOpen => self.confirm_selection(),
            // A newly opened chat starts at its first topic
            Action::LoadForumTopics => self.select(None),
            _ => {}
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect) -> std::io::Result<()> {
        let app_context = Arc::clone(&self.app_context);
        let theme = app_context.theme();
        self.refresh();

        let block = pane::list_block(theme, pane::title("Topics"), self.focused);
        let inner = block.inner(area);
        self.last_list_area = Some(inner);
        if self.visible_topics.is_empty() {
            let placeholder = Paragraph::new(Line::styled(
                "Loading topics…",
                theme.style(HighlightGroup::Placeholder),
            ))
            .block(block);
            frame.render_widget(placeholder, area);
            return Ok(());
        }

        let width = usize::from(inner.width);
        let selected = self.topic_list_state.selected();
        let open = app_context.store().open_topic_id();
        let items = self
            .visible_topics
            .iter()
            .enumerate()
            .map(|(index, topic)| {
                topic_list_item_text(
                    topic,
                    &app_context,
                    width,
                    selected == Some(index),
                    open == Some(topic.topic_id),
                )
            });
        let list = List::new(items)
            .block(block)
            .direction(ListDirection::TopToBottom);
        frame.render_stateful_widget(list, area, &mut self.topic_list_state);
        self.last_list_offset = Some(self.topic_list_state.offset());
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/topic_list_window.rs"]
mod tests;
