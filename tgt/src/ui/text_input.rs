//! A one-line text input for popups: typing, a cursor and the usual editing
//! keys, drawn scrolled to keep the cursor in view.

use crate::{
    app::action::Modifiers,
    settings::{BorderSurface, HighlightGroup, Theme},
};
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Position, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use unicode_width::UnicodeWidthStr;

/// Widest an input popup gets, borders included.
const MODAL_WIDTH: u16 = 56;

/// Draws a popup centered in `area` with `input` after `prompt` and
/// `status` (a hint, progress or an error) under it; the cursor is placed in
/// the input when `focused`. Returns the popup's bounds.
#[allow(clippy::too_many_arguments)]
pub fn draw_modal(
    frame: &mut Frame<'_>,
    area: Rect,
    theme: &Theme,
    title: &str,
    prompt: &str,
    input: &TextInput,
    status: Span<'static>,
    focused: bool,
) -> Rect {
    let width = MODAL_WIDTH.max(title.width() as u16 + 6).min(area.width);
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
            format!(" {title} "),
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
    let prompt_width = prompt.width();
    let (shown, column) = input.visible(usize::from(content.width).saturating_sub(prompt_width));
    let lines = vec![
        Line::from(vec![
            Span::styled(prompt.to_owned(), theme.style(HighlightGroup::FieldLabel)),
            Span::raw(shown),
        ]),
        Line::from(status),
    ];
    frame.render_widget(Paragraph::new(lines), content);
    if focused {
        let x = content.x + (prompt_width + column) as u16;
        frame.set_cursor_position(Position::new(x.min(content.right()), content.y));
    }
    popup
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextInput {
    text: String,
    /// The cursor, in characters into `text`.
    cursor: usize,
}

impl TextInput {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    /// The byte offset of the cursor.
    fn cursor_byte(&self) -> usize {
        self.text
            .char_indices()
            .nth(self.cursor)
            .map_or(self.text.len(), |(i, _)| i)
    }

    fn len(&self) -> usize {
        self.text.chars().count()
    }

    /// Inserts `text` at the cursor, leaving out line breaks and other
    /// control characters: the input is one line.
    pub fn insert(&mut self, text: &str) {
        let text: String = text.chars().filter(|c| !c.is_control()).collect();
        let at = self.cursor_byte();
        self.text.insert_str(at, &text);
        self.cursor += text.chars().count();
    }

    /// Handles an editing key; whether the text changed.
    pub fn handle_key(&mut self, key: KeyCode, modifiers: Modifiers) -> bool {
        let before = self.text.len();
        match key {
            // Clear up to the cursor, like a shell
            KeyCode::Char('u') if modifiers.control => {
                self.text.drain(..self.cursor_byte());
                self.cursor = 0;
            }
            KeyCode::Char(c) if !modifiers.control && !modifiers.alt => {
                self.insert(c.encode_utf8(&mut [0; 4]));
            }
            KeyCode::Backspace if self.cursor > 0 => {
                self.cursor -= 1;
                let at = self.cursor_byte();
                self.text.remove(at);
            }
            KeyCode::Delete if self.cursor < self.len() => {
                let at = self.cursor_byte();
                self.text.remove(at);
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.len()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.len(),
            _ => {}
        }
        self.text.len() != before
    }

    /// The text shown in `width` columns, scrolled so the cursor is in view,
    /// and the cursor's column in it.
    pub fn visible(&self, width: usize) -> (String, usize) {
        let before: Vec<char> = self.text.chars().take(self.cursor).collect();
        let mut skip = 0;
        while before[skip..].iter().collect::<String>().width() > width.saturating_sub(1)
            && skip < before.len()
        {
            skip += 1;
        }
        let column = before[skip..].iter().collect::<String>().width();
        (self.text.chars().skip(skip).collect(), column)
    }
}

#[cfg(test)]
#[path = "tests/text_input.rs"]
mod tests;
