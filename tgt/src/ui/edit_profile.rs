//! Popup to edit your own profile: first and last name, bio and username.

use crate::{
    app::action::Action,
    app::context::AppContext,
    settings::{BorderSurface, HighlightGroup},
    ui::component_traits::{Component, HandleFocus},
    ui::text_input::TextInput,
};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use libtgt::model::{ProfileEdit, ProfileField};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::{io, sync::Arc};
use tokio::sync::mpsc::UnboundedSender;
use unicode_width::UnicodeWidthStr;

const TITLE: &str = "Edit your profile";
/// Widest the popup gets, borders included.
const MAX_WIDTH: u16 = 64;
/// Shown under the fields until something else is.
const HINT: &str = "tab next field, enter saves, esc closes";

/// What's under the fields.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Status {
    /// Your bio is being loaded; the fields can't be edited yet.
    Loading,
    Hint,
    Saving,
    Error(String),
}

pub struct EditProfile {
    app_context: Arc<AppContext>,
    name: String,
    action_tx: Option<UnboundedSender<Action>>,
    focused: bool,
    /// One input per field, in [`ProfileField::ALL`] order.
    inputs: [TextInput; 4],
    field: ProfileField,
    /// The profile as it was when the fields were filled in.
    original: ProfileEdit,
    status: Status,
    /// Popup bounds from the most recent draw.
    last_popup: Option<Rect>,
}

fn index(field: ProfileField) -> usize {
    ProfileField::ALL
        .iter()
        .position(|f| *f == field)
        .unwrap_or(0)
}

impl EditProfile {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        EditProfile {
            app_context,
            name: String::new(),
            action_tx: None,
            focused: false,
            inputs: Default::default(),
            field: ProfileField::FirstName,
            original: ProfileEdit::default(),
            status: Status::Loading,
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

    /// The profile as the fields have it now.
    fn edit(&self) -> ProfileEdit {
        let mut edit = ProfileEdit::default();
        for field in ProfileField::ALL {
            edit.set(field, self.inputs[index(field)].text());
        }
        edit
    }

    /// Fills the fields in from what the store knows of your profile.
    fn fill(&mut self) {
        let profile = self
            .app_context
            .store()
            .my_profile_edit()
            .unwrap_or_default();
        self.fill_with(profile);
    }

    fn fill_with(&mut self, profile: ProfileEdit) {
        self.original = profile;
        for field in ProfileField::ALL {
            self.inputs[index(field)].set(self.original.get(field));
        }
    }

    fn show(&mut self) {
        self.field = ProfileField::FirstName;
        self.fill();
        // Your bio is reloaded either way; with it known already, editing
        // can start right away
        self.status = if self.app_context.store().my_full_info_loaded() {
            Status::Hint
        } else {
            Status::Loading
        };
    }

    /// Your full info has been (re)loaded: fill the fields in again unless
    /// they've been edited meanwhile.
    fn loaded(&mut self) {
        if self.status == Status::Loading || self.edit() == self.original {
            self.fill();
        }
        if self.status == Status::Loading {
            self.status = Status::Hint;
        }
    }

    fn failed(&mut self, field: Option<ProfileField>, message: String) {
        if self.status == Status::Loading {
            // Without the bio, the rest can still be edited; an untouched
            // bio isn't saved
            self.fill();
        }
        if let Some(field) = field {
            self.field = field;
        }
        self.status = Status::Error(message);
    }

    /// Saves what changed, unless something can't be saved.
    fn confirm(&mut self) {
        if matches!(self.status, Status::Loading | Status::Saving) {
            return;
        }
        let edit = self.edit();
        if let Some((field, message)) = edit.problem() {
            self.field = field;
            self.status = Status::Error(message);
            return;
        }
        let changes = edit.changes(&self.original);
        if changes.is_empty() {
            self.send(Action::HideEditProfile);
            return;
        }
        self.status = Status::Saving;
        self.send(Action::SaveProfile(changes));
    }

    fn editable(&self) -> bool {
        !matches!(self.status, Status::Loading | Status::Saving)
    }

    fn input(&mut self) -> &mut TextInput {
        &mut self.inputs[index(self.field)]
    }

    #[cfg(test)]
    fn text(&self, field: ProfileField) -> &str {
        self.inputs[index(field)].text()
    }
}

impl HandleFocus for EditProfile {
    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl Component for EditProfile {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> io::Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn update(&mut self, action: Action) {
        match action {
            Action::ShowEditProfile => self.show(),
            Action::HideEditProfile => self.status = Status::Hint,
            Action::EditProfileLoaded => self.loaded(),
            Action::EditProfileFailed(field, message) => self.failed(field, message),
            Action::EditProfileNextField if self.status != Status::Saving => {
                self.field = self.field.next();
            }
            Action::EditProfilePreviousField if self.status != Status::Saving => {
                self.field = self.field.previous();
            }
            Action::EditProfileConfirm => self.confirm(),
            Action::Key(key, modifiers) if self.editable() => {
                if self.input().handle_key(key, modifiers) {
                    self.status = Status::Hint;
                }
            }
            Action::Paste(text) if self.editable() => {
                self.input().insert(&text);
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
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return Ok(None);
        }
        if !inside {
            return Ok(Some(Action::HideEditProfile));
        }
        // A click on a field's row moves to it; the fields start a row in
        if self.status != Status::Saving
            && let Some(field) = usize::from(mouse.row.saturating_sub(popup.y))
                .checked_sub(1)
                .and_then(|row| ProfileField::ALL.get(row))
        {
            self.field = *field;
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect) -> io::Result<()> {
        let theme = self.app_context.theme();
        let width = MAX_WIDTH.min(area.width);
        // A row per field and the status, inside the borders
        let height = (ProfileField::ALL.len() as u16 + 4).min(area.height);
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
        let label_width = ProfileField::ALL
            .iter()
            .map(|f| f.label().width())
            .max()
            .unwrap_or(0);
        // The label, two spaces, and `@` before the username
        let prefix_width = label_width + 2;
        let input_width = usize::from(content.width).saturating_sub(prefix_width + 1);
        let mut lines = Vec::new();
        let mut cursor = None;
        for (row, field) in ProfileField::ALL.into_iter().enumerate() {
            let active = field == self.field;
            let label_style = theme.style(if active {
                HighlightGroup::ActiveField
            } else {
                HighlightGroup::FieldLabel
            });
            let (shown, column) = self.inputs[row].visible(input_width);
            let at = if field == ProfileField::Username {
                "@"
            } else {
                " "
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{:>label_width$}  ", field.label()), label_style),
                Span::styled(at, theme.style(HighlightGroup::Muted)),
                Span::raw(shown),
            ]));
            if active {
                cursor = Some((row as u16, prefix_width + 1 + column));
            }
        }
        let status = match &self.status {
            Status::Loading => Span::styled("Loading…", theme.style(HighlightGroup::Loading)),
            Status::Hint => Span::styled(HINT, theme.style(HighlightGroup::Hint)),
            Status::Saving => Span::styled("Saving…", theme.style(HighlightGroup::Loading)),
            Status::Error(message) => {
                Span::styled(message.clone(), theme.style(HighlightGroup::Warning))
            }
        };
        lines.push(Line::from(""));
        lines.push(Line::from(status));
        frame.render_widget(Paragraph::new(lines), content);
        if self.focused
            && self.editable()
            && let Some((row, column)) = cursor
        {
            let x = (content.x + column as u16).min(content.right());
            frame.set_cursor_position(Position::new(x, content.y + row));
        }
        self.last_popup = Some(popup);
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/edit_profile.rs"]
mod tests;
