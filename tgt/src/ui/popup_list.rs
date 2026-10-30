//! Selectable list popups like concord's: a centered modal with a title, the
//! selection marker before the highlighted row and a column of keys.

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    settings::{BorderSurface, HighlightGroup, Theme},
    ui::pane,
};

/// One row of a popup list.
#[derive(Debug, Clone, Default)]
pub struct PopupRow {
    /// The key that picks the row, shown before the label.
    pub key: String,
    pub label: Vec<Span<'static>>,
    /// Dimmed, still selectable so pressing it can say why it's unavailable.
    pub disabled: bool,
}

/// The first row shown so that `selected` is visible in `height` rows.
pub fn scroll_offset(selected: usize, len: usize, height: usize) -> usize {
    // Nothing selected (e.g. the leader menu): from the top
    if height == 0 || len <= height || selected >= len {
        return 0;
    }
    selected.saturating_sub(height - 1).min(len - height)
}

/// The lines of the rows, without the frame.
pub fn lines(theme: &Theme, rows: &[PopupRow], selected: usize) -> Vec<Line<'static>> {
    let key_width = rows.iter().map(|row| row.key.width()).max().unwrap_or(0);
    rows.iter()
        .enumerate()
        .map(|(index, row)| {
            let is_selected = index == selected;
            let mut spans = vec![pane::selection_marker(theme, is_selected)];
            if key_width > 0 {
                let padding = " ".repeat(key_width - row.key.width());
                spans.push(Span::styled(
                    format!("{padding}{} ", row.key),
                    theme.style(HighlightGroup::Shortcut),
                ));
            }
            for span in &row.label {
                let mut span = span.clone();
                if row.disabled {
                    span.style = theme.apply(HighlightGroup::Disabled, span.style);
                }
                spans.push(span);
            }
            let line = Line::from(spans);
            if is_selected {
                line.style(theme.style(HighlightGroup::SelectedRow))
            } else {
                line
            }
        })
        .collect()
}

/// Draws `rows` in a modal centered in `area`, at least `min_width` wide,
/// scrolled to keep `selected` in view.
pub fn draw(
    frame: &mut Frame<'_>,
    area: Rect,
    theme: &Theme,
    title: &str,
    rows: &[PopupRow],
    selected: usize,
    min_width: u16,
) {
    let all_lines = lines(theme, rows, selected);
    let content_width = all_lines.iter().map(Line::width).max().unwrap_or(0) as u16;
    let width = (content_width + 4)
        .max(min_width)
        .max(title.width() as u16 + 6)
        .min(area.width);
    let height = (all_lines.len() as u16 + 2).min(area.height);
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let visible = usize::from(height.saturating_sub(2));
    let offset = scroll_offset(selected, all_lines.len(), visible);

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
    let shown: Vec<Line<'static>> = all_lines.into_iter().skip(offset).take(visible).collect();
    // A column of padding on both sides, like concord's modals
    let content = Rect::new(
        inner.x + 1,
        inner.y,
        inner.width.saturating_sub(2),
        inner.height,
    );
    frame.render_widget(Paragraph::new(shown).style(Style::default()), content);
}
