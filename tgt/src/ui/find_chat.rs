//! Popup to open a chat by its public username: a person's, bot's, group's
//! or channel's, whether or not it's in the chat list yet.

use crate::{
    app::action::{Action, Modifiers},
    app::context::AppContext,
    settings::{BorderSurface, HighlightGroup},
    ui::component_traits::{Component, HandleFocus},
};
use crossterm::event::{KeyCode, MouseButton, MouseEvent, MouseEventKind};
use libtgt::model::parse_username;
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
const WIDTH: u16 = 52;
const TITLE: &str = "Open chat by username";
/// Shown under the input until something else is.
const HINT: &str = "enter opens the chat, esc closes";
/// Shown when what was typed can't be a username.
pub const INVALID: &str = "A username has 4-32 letters, digits or _";

/// What to tell when TDLib can't find the chat of `@username`.
pub fn failure_message(username: &str, error: &str) -> String {
    if error.contains("NOT_OCCUPIED") || error.contains("not found") {
        format!("No one is @{username}")
    } else if error.contains("INVALID") {
        format!("@{username} isn't a valid username")
    } else {
        format!("Can't find @{username}: {error}")
    }
}

/// What's under the input.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Status {
    Hint,
    Searching,
    Error(String),
}

pub struct FindChat {
    app_context: Arc<AppContext>,
    name: String,
    action_tx: Option<UnboundedSender<Action>>,
    focused: bool,
    input: String,
    /// The cursor, in characters into `input`.
    cursor: usize,
    status: Status,
    /// Popup bounds from the most recent draw.
    last_popup: Option<Rect>,
}

impl FindChat {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        FindChat {
            app_context,
            name: String::new(),
            action_tx: None,
            focused: false,
            input: String::new(),
            cursor: 0,
            status: Status::Hint,
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

    fn reset(&mut self) {
        self.input.clear();
        self.cursor = 0;
        self.status = Status::Hint;
    }

    /// The byte offset of the cursor.
    fn cursor_byte(&self) -> usize {
        self.input
            .char_indices()
            .nth(self.cursor)
            .map_or(self.input.len(), |(i, _)| i)
    }

    fn insert(&mut self, text: &str) {
        // A username is one line
        let text: String = text.chars().filter(|c| !c.is_control()).collect();
        let at = self.cursor_byte();
        self.input.insert_str(at, &text);
        self.cursor += text.chars().count();
        self.status = Status::Hint;
    }

    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor -= 1;
        let at = self.cursor_byte();
        self.input.remove(at);
        self.status = Status::Hint;
    }

    fn delete(&mut self) {
        if self.cursor < self.input.chars().count() {
            let at = self.cursor_byte();
            self.input.remove(at);
            self.status = Status::Hint;
        }
    }

    /// Looks the username up, unless it can't be one.
    fn confirm(&mut self) {
        if self.status == Status::Searching {
            return;
        }
        match parse_username(&self.input) {
            Some(username) => {
                self.status = Status::Searching;
                self.send(Action::FindChat(username));
            }
            None => self.status = Status::Error(INVALID.to_owned()),
        }
    }

    fn handle_key(&mut self, key: KeyCode, modifiers: Modifiers) {
        match key {
            KeyCode::Char('u') if modifiers.control => {
                self.input.drain(..self.cursor_byte());
                self.cursor = 0;
            }
            KeyCode::Char(c) if !modifiers.control && !modifiers.alt => {
                self.insert(c.encode_utf8(&mut [0; 4]));
            }
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.input.chars().count()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.input.chars().count(),
            _ => {}
        }
    }
}

impl HandleFocus for FindChat {
    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl Component for FindChat {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> io::Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn update(&mut self, action: Action) {
        match action {
            Action::ShowFindChat | Action::HideFindChat => self.reset(),
            Action::FindChatConfirm => self.confirm(),
            Action::FindChatFailed(message) => self.status = Status::Error(message),
            Action::Key(key, modifiers) if self.status != Status::Searching => {
                self.handle_key(key, modifiers)
            }
            Action::Paste(text) if self.status != Status::Searching => self.insert(&text),
            _ => {}
        }
    }

    fn handle_mouse_events(&mut self, mouse: MouseEvent) -> io::Result<Option<Action>> {
        let Some(popup) = self.last_popup else {
            return Ok(None);
        };
        let inside = mouse.column >= popup.x
            && mouse.column < popup.right()
            && mouse.row >= popup.y
            && mouse.row < popup.bottom();
        if mouse.kind == MouseEventKind::Down(MouseButton::Left) && !inside {
            return Ok(Some(Action::HideFindChat));
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect) -> io::Result<()> {
        let theme = self.app_context.theme();
        let width = WIDTH.min(area.width);
        let height = 4.min(area.height);
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
                format!(" {TITLE} "),
                theme.style(HighlightGroup::ModalTitle),
            ))
            .style(theme.style(HighlightGroup::Normal));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);
        // A column of padding on both sides, like the other modals
        let content = Rect::new(
            inner.x + 1,
            inner.y,
            inner.width.saturating_sub(2),
            inner.height,
        );

        // Keep the cursor in view in a long input
        let before: String = self.input.chars().take(self.cursor).collect();
        let room = usize::from(content.width).saturating_sub(2);
        let mut skip = 0;
        while before.chars().skip(skip).collect::<String>().width() > room {
            skip += 1;
        }
        let shown: String = self.input.chars().skip(skip).collect();
        let status = match &self.status {
            Status::Hint => Span::styled(HINT, theme.style(HighlightGroup::Hint)),
            Status::Searching => Span::styled("Searching…", theme.style(HighlightGroup::Loading)),
            Status::Error(message) => {
                Span::styled(message.clone(), theme.style(HighlightGroup::Warning))
            }
        };
        let lines = vec![
            Line::from(vec![
                Span::styled("@ ", theme.style(HighlightGroup::FieldLabel)),
                Span::raw(shown),
            ]),
            Line::from(status),
        ];
        frame.render_widget(Paragraph::new(lines), content);
        if self.focused {
            let x = content.x + 2 + before.chars().skip(skip).collect::<String>().width() as u16;
            frame.set_cursor_position(Position::new(x.min(content.right()), content.y));
        }
        self.last_popup = Some(popup);
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/find_chat.rs"]
mod tests;
