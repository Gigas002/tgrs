//! Popup to forward messages: pick the chat to send them to, found by name,
//! and whether to keep the original author.

use crate::{
    app::action::{Action, ForwardRequest},
    app::context::AppContext,
    settings::{BorderSurface, HighlightGroup},
    ui::component_traits::{Component, HandleFocus},
    ui::text_input::TextInput,
};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use libtgrs::model::ChatListEntry;
use nucleo_matcher::{Config, Matcher, Utf32Str};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::{io, sync::Arc};
use tokio::sync::mpsc::UnboundedSender;
use unicode_width::UnicodeWidthStr;

/// Widest the popup gets, borders included.
const MAX_WIDTH: u16 = 60;
/// Most chats shown at once.
const MAX_ROWS: usize = 12;
const HINT: &str = "enter forwards, tab toggles the author, esc closes";

/// The indices of `names` matching `query`, best first; all of them in
/// their order for an empty query.
pub fn matching(names: &[&str], query: &str) -> Vec<usize> {
    let query = query.trim();
    if query.is_empty() {
        return (0..names.len()).collect();
    }
    let mut config = Config::DEFAULT;
    config.prefer_prefix = true;
    let mut matcher = Matcher::new(config);
    let query: Vec<char> = query.chars().collect();
    let mut scored: Vec<(usize, u16)> = names
        .iter()
        .enumerate()
        .filter_map(|(index, name)| {
            let name: Vec<char> = name.chars().collect();
            matcher
                .fuzzy_indices(
                    Utf32Str::Unicode(&name),
                    Utf32Str::Unicode(&query),
                    &mut Vec::new(),
                )
                .map(|score| (index, score))
        })
        .collect();
    // Stable, so equally good matches keep the chat list order
    scored.sort_by_key(|(_, score)| std::cmp::Reverse(*score));
    scored.into_iter().map(|(index, _)| index).collect()
}

pub struct Forward {
    app_context: Arc<AppContext>,
    name: String,
    action_tx: Option<UnboundedSender<Action>>,
    focused: bool,
    visible: bool,
    from_chat_id: i64,
    message_ids: Vec<i64>,
    input: TextInput,
    /// The chats you can forward to: the chat list, then the archive.
    chats: Vec<ChatListEntry>,
    /// Indices into `chats` of those matching the input, best first.
    shown: Vec<usize>,
    selected: usize,
    /// Forward as copies, without the original author.
    send_copy: bool,
    /// Popup bounds, first list row and first shown chat from the most
    /// recent draw.
    last_popup: Option<(Rect, u16, usize)>,
}

impl Forward {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Forward {
            app_context,
            name: String::new(),
            action_tx: None,
            focused: false,
            visible: false,
            from_chat_id: 0,
            message_ids: Vec::new(),
            input: TextInput::default(),
            chats: Vec::new(),
            shown: Vec::new(),
            selected: 0,
            send_copy: false,
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

    fn show(&mut self, from_chat_id: i64, message_ids: Vec<i64>) {
        let mut chats = self.app_context.store().chat_list();
        chats.extend(self.app_context.store().archived_chat_list());
        self.set_chats(chats);
        self.from_chat_id = from_chat_id;
        self.message_ids = message_ids;
        self.send_copy = false;
        self.visible = true;
    }

    fn set_chats(&mut self, chats: Vec<ChatListEntry>) {
        self.chats = chats;
        self.input.clear();
        self.refilter();
    }

    fn refilter(&mut self) {
        let names: Vec<&str> = self.chats.iter().map(|c| c.chat_name.as_str()).collect();
        self.shown = matching(&names, self.input.text());
        self.selected = 0;
    }

    fn move_selection(&mut self, forward: bool) {
        let len = self.shown.len();
        if len > 0 {
            self.selected = if forward {
                (self.selected + 1) % len
            } else {
                (self.selected + len - 1) % len
            };
        }
    }

    fn selected_chat(&self) -> Option<&ChatListEntry> {
        self.shown
            .get(self.selected)
            .and_then(|index| self.chats.get(*index))
    }

    fn confirm(&mut self) {
        let Some(chat) = self.selected_chat() else {
            return;
        };
        let request = ForwardRequest {
            to_chat_id: chat.chat_id,
            to_chat_name: chat.chat_name.clone(),
            from_chat_id: self.from_chat_id,
            message_ids: self.message_ids.clone(),
            send_copy: self.send_copy,
        };
        self.send(Action::HideForward);
        self.send(Action::ForwardMessages(request));
    }

    fn title(&self) -> String {
        match self.message_ids.len() {
            1 => "Forward message".to_owned(),
            n => format!("Forward {n} messages"),
        }
    }

    #[cfg(test)]
    fn shown_names(&self) -> Vec<&str> {
        self.shown
            .iter()
            .map(|index| self.chats[*index].chat_name.as_str())
            .collect()
    }
}

impl HandleFocus for Forward {
    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl Component for Forward {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> io::Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn update(&mut self, action: Action) {
        match action {
            Action::ShowForward(from_chat_id, message_ids) => self.show(from_chat_id, message_ids),
            Action::HideForward => {
                self.visible = false;
                self.chats.clear();
                self.shown.clear();
            }
            Action::ForwardNext => self.move_selection(true),
            Action::ForwardPrevious => self.move_selection(false),
            Action::ForwardToggleCopy => self.send_copy = !self.send_copy,
            Action::ForwardConfirm => self.confirm(),
            Action::Key(key, modifiers) if self.visible => {
                if self.input.handle_key(key, modifiers) {
                    self.refilter();
                }
            }
            Action::Paste(text) if self.visible => {
                self.input.insert(&text);
                self.refilter();
            }
            _ => {}
        }
    }

    fn handle_mouse_events(&mut self, mouse: MouseEvent) -> io::Result<Option<Action>> {
        let Some((popup, list_y, offset)) = self.last_popup else {
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
                    return Ok(Some(Action::HideForward));
                }
                if mouse.row >= list_y {
                    let index = offset + usize::from(mouse.row - list_y);
                    if index < self.shown.len().min(offset + MAX_ROWS) {
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
        let width = MAX_WIDTH.min(area.width);
        // Input, option, a blank line, the chats, a blank line and the hint
        let rows = self.shown.len().clamp(1, MAX_ROWS);
        let height = (rows as u16 + 7).min(area.height);
        let popup = Rect::new(
            area.x + area.width.saturating_sub(width) / 2,
            area.y + area.height.saturating_sub(height) / 2,
            width,
            height,
        );
        frame.render_widget(Clear, popup);
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(theme.border_type(BorderSurface::Modal))
            .border_style(theme.style(HighlightGroup::ModalBorder))
            .title(Span::styled(
                format!(" {} ", self.title()),
                theme.style(HighlightGroup::ModalTitle),
            ))
            .style(theme.style(HighlightGroup::Normal));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);
        let content = Rect::new(
            inner.x + 1,
            inner.y,
            inner.width.saturating_sub(2),
            inner.height,
        );
        let prompt = "To: ";
        let (shown_text, column) = self
            .input
            .visible(usize::from(content.width).saturating_sub(prompt.width()));
        let check = if self.send_copy { "[x]" } else { "[ ]" };
        let mut lines = vec![
            Line::from(vec![
                Span::styled(prompt, theme.style(HighlightGroup::FieldLabel)),
                Span::raw(shown_text),
            ]),
            Line::from(Span::styled(
                format!("{check} without the original author"),
                theme.style(HighlightGroup::Description),
            )),
            Line::from(""),
        ];
        let offset = (self.selected + 1).saturating_sub(MAX_ROWS);
        if self.shown.is_empty() {
            lines.push(Line::styled(
                "No chat matches",
                theme.style(HighlightGroup::Placeholder),
            ));
        }
        for (row, index) in self.shown.iter().enumerate().skip(offset).take(MAX_ROWS) {
            let chat = &self.chats[*index];
            let style = if row == self.selected {
                theme.style(HighlightGroup::SelectedRow)
            } else {
                theme.style(HighlightGroup::Normal)
            };
            lines.push(Line::styled(
                format!("{} {}", chat.kind.icon(), chat.chat_name),
                style,
            ));
        }
        lines.push(Line::from(""));
        lines.push(Line::styled(HINT, theme.style(HighlightGroup::Hint)));
        frame.render_widget(Paragraph::new(lines), content);
        if self.focused {
            let x = content.x + (prompt.width() + column) as u16;
            frame.set_cursor_position(Position::new(x.min(content.right()), content.y));
        }
        self.last_popup = Some((popup, content.y + 3, offset));
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/forward.rs"]
mod tests;
