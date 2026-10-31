//! Popup to pick a sticker to send: the sticker sets as tabs (the recent
//! stickers first) over a grid of their stickers, drawn as images with the
//! `media-preview` feature and as their emoji otherwise.

use std::{collections::HashSet, io, sync::Arc};

use libtgt::model::{StickerEntry, StickerSetEntry};
use ratatui::{
    Frame,
    layout::{Alignment, Margin, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use tokio::sync::mpsc::UnboundedSender;
use unicode_width::UnicodeWidthStr;

use crate::{
    app::action::Action,
    app::context::AppContext,
    settings::{BorderSurface, HighlightGroup},
    ui::component_traits::{Component, HandleFocus},
};

/// Columns a sticker takes in the grid, its frame included.
pub const CELL_WIDTH: u16 = 12;
/// Rows a sticker takes in the grid, its frame included.
pub const CELL_HEIGHT: u16 = 6;
/// The popup's widest and tallest size.
const MAX_WIDTH: u16 = 4 * 2 + 6 * CELL_WIDTH;
const MAX_HEIGHT: u16 = 6 + 4 * CELL_HEIGHT;
/// Sticker images kept decoded: a few sets' worth.
#[cfg(feature = "media-preview")]
const PREVIEW_CACHE_SIZE: usize = 160;

/// Picks a sticker to send to the open chat.
pub struct StickerPicker {
    app_context: Arc<AppContext>,
    name: String,
    action_tx: Option<UnboundedSender<Action>>,
    focused: bool,
    /// The sticker sets, kept between openings; None until first loaded.
    sets: Option<Vec<StickerSetEntry>>,
    /// The shown set.
    set_index: usize,
    /// The selected sticker of the shown set.
    selected: usize,
    /// The first grid row shown.
    scroll_row: usize,
    /// Stickers per grid row at the last draw.
    columns: usize,
    /// Sets whose stickers were asked for since the picker opened.
    requested: HashSet<i64>,
    #[cfg(feature = "media-preview")]
    previews: crate::ui::preview::ChatPreviews,
}

impl StickerPicker {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        StickerPicker {
            app_context,
            name: String::new(),
            action_tx: None,
            focused: false,
            sets: None,
            set_index: 0,
            selected: 0,
            scroll_row: 0,
            columns: 1,
            requested: HashSet::new(),
            #[cfg(feature = "media-preview")]
            previews: crate::ui::preview::ChatPreviews::with_capacity(PREVIEW_CACHE_SIZE),
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

    fn current_set(&self) -> Option<&StickerSetEntry> {
        self.sets.as_ref()?.get(self.set_index)
    }

    fn stickers(&self) -> &[StickerEntry] {
        self.current_set().map_or(&[], |set| &set.stickers)
    }

    /// The selected sticker.
    pub fn selected_sticker(&self) -> Option<&StickerEntry> {
        self.stickers().get(self.selected)
    }

    /// Asks for the shown set's stickers unless they're loaded or asked for.
    fn load_current_set(&mut self) {
        let Some(set) = self.current_set() else {
            return;
        };
        let id = set.id;
        if !set.is_loaded() && self.requested.insert(id) {
            self.send(Action::LoadStickerSet(id));
        }
    }

    /// The new sets keep the stickers already loaded, except the recent ones,
    /// which change with each sticker sent. The shown set stays shown.
    fn merge_sets(&mut self, mut sets: Vec<StickerSetEntry>) {
        let shown = self.current_set().map(|set| set.id);
        if let Some(old) = self.sets.take() {
            for set in &mut sets {
                if let Some(loaded) = old.iter().find(|old| {
                    old.id == set.id && old.id != StickerSetEntry::RECENT && old.is_loaded()
                }) {
                    set.stickers = loaded.stickers.clone();
                    set.size = loaded.size;
                }
            }
        }
        let index = shown.and_then(|id| sets.iter().position(|set| set.id == id));
        if index != Some(self.set_index) {
            self.set_index = index.unwrap_or(0);
            self.selected = 0;
            self.scroll_row = 0;
        }
        self.sets = Some(sets);
        self.selected = self.selected.min(self.stickers().len().saturating_sub(1));
        self.load_current_set();
    }

    fn set_loaded(&mut self, set: StickerSetEntry) {
        self.requested.remove(&set.id);
        if let Some(old) = self
            .sets
            .as_mut()
            .and_then(|sets| sets.iter_mut().find(|old| old.id == set.id))
        {
            *old = set;
        }
    }

    /// Shows the set `step` sets away, going around.
    fn switch_set(&mut self, step: isize) {
        let len = self.sets.as_ref().map_or(0, Vec::len);
        if len == 0 {
            return;
        }
        let len = len as isize;
        self.set_index = (self.set_index as isize + step).rem_euclid(len) as usize;
        self.selected = 0;
        self.scroll_row = 0;
        self.load_current_set();
    }

    /// Moves the selection `step` stickers, stopping at the ends.
    fn move_selection(&mut self, step: isize) {
        let len = self.stickers().len();
        if len == 0 {
            return;
        }
        let target = self.selected as isize + step;
        // A row up or down that doesn't exist: stay
        if !(0..len as isize).contains(&target) && step.unsigned_abs() > 1 {
            let last_row_start = (len - 1) / self.columns * self.columns;
            if step > 0 && self.selected < last_row_start {
                self.selected = len - 1;
            }
            return;
        }
        self.selected = target.clamp(0, len as isize - 1) as usize;
    }

    /// Closes the picker and sends the selected sticker.
    fn pick(&mut self) {
        let Some(sticker) = self.selected_sticker().cloned() else {
            return;
        };
        self.send(Action::HideStickerPicker);
        self.send(Action::SendSticker(sticker));
    }

    /// The set titles on one line of `width` columns, scrolled to show the
    /// current one.
    fn tabs(&self, width: usize) -> Line<'static> {
        let theme = self.app_context.theme();
        let Some(sets) = self.sets.as_ref() else {
            return Line::default();
        };
        let separator = " · ";
        let titles: Vec<String> = sets.iter().map(|set| format!(" {} ", set.title)).collect();
        // Drop sets on the left until the current one fits
        let mut first = 0;
        while first < self.set_index {
            let used: usize = titles[first..=self.set_index]
                .iter()
                .map(|title| title.width() + separator.width())
                .sum();
            if used <= width {
                break;
            }
            first += 1;
        }
        let mut spans = Vec::new();
        if first > 0 {
            spans.push(Span::styled("‹ ", theme.style(HighlightGroup::Muted)));
        }
        for (index, title) in titles.iter().enumerate().skip(first) {
            if index > first {
                spans.push(Span::styled(
                    separator,
                    theme.style(HighlightGroup::Decoration),
                ));
            }
            let group = if index == self.set_index {
                HighlightGroup::SelectedRow
            } else {
                HighlightGroup::Muted
            };
            spans.push(Span::styled(title.clone(), theme.style(group)));
        }
        Line::from(spans)
    }

    /// Keeps the selected sticker's row among the `rows` shown.
    fn scroll_to_selection(&mut self, rows: usize) {
        let row = self.selected / self.columns;
        if row < self.scroll_row {
            self.scroll_row = row;
        } else if rows > 0 && row >= self.scroll_row + rows {
            self.scroll_row = row + 1 - rows;
        }
    }

    /// Draws the sticker at `index` in `cell`.
    fn draw_sticker(&mut self, frame: &mut Frame<'_>, index: usize, cell: Rect) {
        let theme = self.app_context.theme();
        let Some(sticker) = self.stickers().get(index).cloned() else {
            return;
        };
        if index == self.selected {
            frame.render_widget(
                Block::new()
                    .borders(Borders::ALL)
                    .border_type(theme.border_type(BorderSurface::Modal))
                    .border_style(theme.style(HighlightGroup::FocusedPaneBorder)),
                cell,
            );
        }
        let inner = cell.inner(Margin::new(1, 1));
        #[cfg(feature = "media-preview")]
        if let Some(file) = sticker.preview.as_ref() {
            if let Some(action) = self.previews.request(0, file) {
                self.send(action);
            }
            if self
                .previews
                .draw_loaded(frame, file.id, inner, Alignment::Center)
            {
                return;
            }
        }
        // No image (yet): the sticker's emoji
        let middle = Rect::new(inner.x, inner.y + inner.height / 2, inner.width, 1);
        frame.render_widget(
            Paragraph::new(sticker.emoji).alignment(Alignment::Center),
            middle,
        );
    }

    #[cfg(test)]
    pub fn sets(&self) -> Option<&[StickerSetEntry]> {
        self.sets.as_deref()
    }
}

impl HandleFocus for StickerPicker {
    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl Component for StickerPicker {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> io::Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn update(&mut self, action: Action) {
        match action {
            // Opens at the recent stickers; the sets are reloaded for them
            Action::ShowStickerPicker => {
                self.set_index = 0;
                self.selected = 0;
                self.scroll_row = 0;
                self.requested.clear();
                self.send(Action::LoadStickerSets);
            }
            Action::StickerSetsLoaded(sets) => self.merge_sets(sets),
            Action::StickerSetLoaded(set) => self.set_loaded(set),
            Action::StickerPickerLeft => self.move_selection(-1),
            Action::StickerPickerRight => self.move_selection(1),
            Action::StickerPickerUp => self.move_selection(-(self.columns as isize)),
            Action::StickerPickerDown => self.move_selection(self.columns as isize),
            Action::StickerPickerNextSet => self.switch_set(1),
            Action::StickerPickerPreviousSet => self.switch_set(-1),
            Action::StickerPickerSend => self.pick(),
            #[cfg(feature = "media-preview")]
            Action::PreviewDecoded(payload) => self.previews.decoded(payload),
            _ => {}
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect) -> io::Result<()> {
        let app_context = Arc::clone(&self.app_context);
        let theme = app_context.theme();
        let width = MAX_WIDTH.min(area.width);
        let height = MAX_HEIGHT.min(area.height);
        let popup = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        frame.render_widget(Clear, popup);
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(theme.border_type(BorderSurface::Modal))
            .border_style(theme.style(HighlightGroup::ModalBorder))
            .title(Span::styled(
                " Stickers ",
                theme.style(HighlightGroup::ModalTitle),
            ))
            .style(theme.style(HighlightGroup::Normal));
        // A column of padding on both sides, like concord's modals
        let inner = block.inner(popup).inner(Margin::new(1, 0));
        frame.render_widget(block, popup);
        if inner.height < 3 {
            return Ok(());
        }

        // Tabs, a blank row, the grid, and the selected sticker's emoji
        let tabs = Rect::new(inner.x, inner.y, inner.width, 1);
        let footer = Rect::new(inner.x, inner.bottom() - 1, inner.width, 1);
        let grid = Rect::new(
            inner.x,
            inner.y + 2,
            inner.width,
            inner.height.saturating_sub(3),
        );
        frame.render_widget(Paragraph::new(self.tabs(usize::from(inner.width))), tabs);

        let placeholder = match self.current_set() {
            None if self.sets.is_none() => Some("Loading sticker sets…"),
            None => Some("No sticker sets installed"),
            Some(set) if set.stickers.is_empty() && !set.is_loaded() => Some("Loading stickers…"),
            Some(set) if set.stickers.is_empty() => Some("This set has no stickers"),
            Some(_) => None,
        };
        if let Some(placeholder) = placeholder {
            frame.render_widget(
                Paragraph::new(placeholder)
                    .style(theme.style(HighlightGroup::Placeholder))
                    .alignment(Alignment::Center),
                Rect::new(grid.x, grid.y + grid.height / 2, grid.width, 1),
            );
            return Ok(());
        }

        self.columns = usize::from((grid.width / CELL_WIDTH).max(1));
        let rows = usize::from(grid.height / CELL_HEIGHT);
        self.scroll_to_selection(rows);
        // Center the grid in the popup
        let left = grid.x + (grid.width - (self.columns as u16 * CELL_WIDTH).min(grid.width)) / 2;
        let len = self.stickers().len();
        for row in 0..rows {
            for column in 0..self.columns {
                let index = (self.scroll_row + row) * self.columns + column;
                if index >= len {
                    break;
                }
                let cell = Rect::new(
                    left + column as u16 * CELL_WIDTH,
                    grid.y + row as u16 * CELL_HEIGHT,
                    CELL_WIDTH.min(grid.width),
                    CELL_HEIGHT,
                );
                self.draw_sticker(frame, index, cell);
            }
        }

        if let Some(sticker) = self.selected_sticker() {
            let footer_line = Line::from(vec![
                Span::raw(sticker.emoji.clone()),
                Span::styled(
                    format!("  {}/{}", self.selected + 1, len),
                    theme.style(HighlightGroup::Muted),
                ),
            ]);
            frame.render_widget(
                Paragraph::new(footer_line).alignment(Alignment::Center),
                footer,
            );
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/sticker_picker.rs"]
mod tests;
