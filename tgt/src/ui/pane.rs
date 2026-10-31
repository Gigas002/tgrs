//! Pane frames like concord's: every pane is fully bordered with its title
//! in the top border, and the focused one has a highlighted border.

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Padding},
};
use unicode_width::UnicodeWidthStr;

use crate::settings::{BorderSurface, HighlightGroup, Theme};

/// The frame of a pane titled `title`.
pub fn block<'a>(theme: &Theme, title: impl Into<Line<'a>>, focused: bool) -> Block<'a> {
    let border = if focused {
        HighlightGroup::FocusedPaneBorder
    } else {
        HighlightGroup::PaneBorder
    };
    Block::new()
        .borders(Borders::ALL)
        .border_type(theme.border_type(BorderSurface::Pane))
        .border_style(theme.style(border))
        .title(title)
        .title_style(theme.style(HighlightGroup::PaneTitle))
        .style(theme.style(HighlightGroup::Normal))
}

/// The frame of a list pane, padded so the rows sit as far from the top and
/// the right as they do from the left (the selection marker's width) and the
/// bottom (the blank spacer after each row).
pub fn list_block<'a>(theme: &Theme, title: impl Into<Line<'a>>, focused: bool) -> Block<'a> {
    let marker = u16::try_from(theme.selection_marker().width()).unwrap_or(0);
    block(theme, title, focused).padding(Padding::new(0, marker, 1, 0))
}

/// The index of the list item at `column`, `row`: `area` is where the items
/// are drawn, each `item_rows` tall, the first one shown being `offset`.
pub fn item_at(area: Rect, column: u16, row: u16, offset: usize, item_rows: u16) -> Option<usize> {
    let inside = column >= area.x
        && column < area.x + area.width
        && row >= area.y
        && row < area.y + area.height;
    inside.then(|| offset + usize::from((row - area.y) / item_rows))
}

/// The frame of the message composer, highlighted while typing.
pub fn composer_block<'a>(theme: &Theme, title: impl Into<Line<'a>>, active: bool) -> Block<'a> {
    let border = if active {
        HighlightGroup::ActiveComposerBorder
    } else {
        HighlightGroup::ComposerBorder
    };
    Block::new()
        .borders(Borders::ALL)
        .border_type(theme.border_type(BorderSurface::Composer))
        .border_style(theme.style(border))
        .title(title)
        .title_style(theme.style(HighlightGroup::ComposerTitle))
        .style(theme.style(HighlightGroup::Normal))
}

/// ` title `, padded like concord's pane titles.
pub fn title(text: &str) -> String {
    if text.is_empty() {
        String::new()
    } else {
        format!(" {text} ")
    }
}

/// The selection marker before a list row, or as many spaces.
pub fn selection_marker(theme: &Theme, selected: bool) -> Span<'static> {
    let marker = theme.selection_marker();
    if selected {
        Span::styled(
            marker.to_owned(),
            theme.style(HighlightGroup::SelectionMarker),
        )
    } else {
        Span::raw(" ".repeat(marker.width()))
    }
}
