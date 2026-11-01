//! Popup to open a chat by its public username: a person's, bot's, group's
//! or channel's, whether or not it's in the chat list yet.

use crate::{
    app::action::Action,
    app::context::AppContext,
    settings::HighlightGroup,
    ui::component_traits::{Component, HandleFocus},
    ui::text_input::{self, TextInput},
};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use libtgt::model::parse_username;
use ratatui::{Frame, layout::Rect, text::Span};
use std::{io, sync::Arc};
use tokio::sync::mpsc::UnboundedSender;

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
    input: TextInput,
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
            input: TextInput::default(),
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
        self.status = Status::Hint;
    }

    /// Looks the username up, unless it can't be one.
    fn confirm(&mut self) {
        if self.status == Status::Searching {
            return;
        }
        match parse_username(self.input.text()) {
            Some(username) => {
                self.status = Status::Searching;
                self.send(Action::FindChat(username));
            }
            None => self.status = Status::Error(INVALID.to_owned()),
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
            // Typing waits for the answer to a lookup
            Action::Key(key, modifiers) if self.status != Status::Searching => {
                if self.input.handle_key(key, modifiers) {
                    self.status = Status::Hint;
                }
            }
            Action::Paste(text) if self.status != Status::Searching => {
                self.input.insert(&text);
                self.status = Status::Hint;
            }
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
        let status = match &self.status {
            Status::Hint => Span::styled(HINT, theme.style(HighlightGroup::Hint)),
            Status::Searching => Span::styled("Searching…", theme.style(HighlightGroup::Loading)),
            Status::Error(message) => {
                Span::styled(message.clone(), theme.style(HighlightGroup::Warning))
            }
        };
        self.last_popup = Some(text_input::draw_modal(
            frame,
            area,
            theme,
            TITLE,
            "@ ",
            &self.input,
            status,
            self.focused,
        ));
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/find_chat.rs"]
mod tests;
