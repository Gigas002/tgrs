use crate::{
    app::action::Action,
    app::context::AppContext,
    settings::ActionBinding,
    ui::component_name::ComponentName,
    ui::component_traits::{Component, HandleFocus},
    ui::popup_list::{self, MouseHit, PopupRow},
};
use crossterm::event::MouseEvent;
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
};
use std::{io, sync::Arc};
use tokio::sync::mpsc::UnboundedSender;

/// One entry of the menu: a `[leader_menu]` key and what it runs.
#[derive(Debug, Clone, PartialEq)]
pub struct LeaderEntry {
    pub key: String,
    pub description: String,
    pub action: Action,
}

/// Where the menu was drawn, for mouse clicks.
#[derive(Debug, Clone, Copy)]
struct Layout {
    popup: Rect,
    /// The first entry shown.
    offset: usize,
    /// In two columns: the entries in the left one, and the screen column
    /// where the right one starts.
    columns: Option<(usize, u16)>,
}

/// The menu opened by the leader key (Space), like concord's: it lists the
/// `[leader_menu]` keys, e.g. `1` to hide the chat list. The run loop runs
/// the pressed key's command and closes the menu; the arrows, the mouse
/// wheel and clicks move to an entry and Enter or a second click runs it.
pub struct LeaderMenu {
    app_context: Arc<AppContext>,
    name: String,
    action_tx: Option<UnboundedSender<Action>>,
    focused: bool,
    visible: bool,
    selected: usize,
    last_layout: Option<Layout>,
}

impl LeaderMenu {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        LeaderMenu {
            app_context,
            name: String::new(),
            action_tx: None,
            focused: false,
            visible: false,
            selected: 0,
            last_layout: None,
        }
    }

    pub fn with_name(mut self, name: impl AsRef<str>) -> Self {
        self.name = name.as_ref().to_string();
        self
    }

    /// Whether `action` moves through the menu rather than being an entry.
    pub fn navigates(action: &Action) -> bool {
        matches!(
            action,
            Action::LeaderMenuPrevious | Action::LeaderMenuNext | Action::LeaderMenuConfirm
        )
    }

    /// Every entry: digits first (the panes), then letters and symbols, then
    /// named keys like Space.
    pub fn entries(&self) -> Vec<LeaderEntry> {
        let Some(section) = self
            .app_context
            .keymap_config()
            .section(ComponentName::LeaderMenu)
        else {
            return Vec::new();
        };
        let mut entries: Vec<LeaderEntry> = section
            .iter()
            .filter_map(|(key, binding)| match binding {
                ActionBinding::Single {
                    action,
                    description,
                } if !Self::navigates(action) && *action != Action::HideLeaderMenu => {
                    Some(LeaderEntry {
                        key: key.to_string(),
                        description: description.clone().unwrap_or_else(|| format!("{action:?}")),
                        action: action.clone(),
                    })
                }
                _ => None,
            })
            .collect();
        entries.sort_by(|a, b| {
            let rank = |key: &str| match key.chars().next() {
                Some(c) if key.len() == 1 && c.is_ascii_digit() => 0,
                _ if key.chars().count() == 1 => 1,
                _ => 2,
            };
            (rank(&a.key), &a.key).cmp(&(rank(&b.key), &b.key))
        });
        entries
    }

    fn move_selection(&mut self, forward: bool) {
        let len = self.entries().len();
        if len == 0 {
            return;
        }
        self.selected = if forward {
            (self.selected + 1) % len
        } else {
            (self.selected + len - 1) % len
        };
    }

    /// Closes the menu and runs the entry at `index`.
    fn activate(&self, index: usize) {
        let (Some(entry), Some(tx)) = (self.entries().into_iter().nth(index), &self.action_tx)
        else {
            return;
        };
        let _ = tx.send(Action::HideLeaderMenu);
        let _ = tx.send(entry.action);
    }

    /// The entry under a click on line `line` at screen column `column`.
    fn entry_at(layout: Layout, line: usize, column: u16) -> usize {
        match layout.columns {
            Some((half, right_x)) if column >= right_x => line + half,
            _ => line,
        }
    }
}

impl HandleFocus for LeaderMenu {
    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl Component for LeaderMenu {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> io::Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn update(&mut self, action: Action) {
        match action {
            Action::ShowLeaderMenu => {
                self.visible = true;
                self.selected = 0;
            }
            Action::HideLeaderMenu => {
                self.visible = false;
                self.last_layout = None;
            }
            Action::LeaderMenuPrevious => self.move_selection(false),
            Action::LeaderMenuNext => self.move_selection(true),
            Action::LeaderMenuConfirm => self.activate(self.selected),
            _ => {}
        }
    }

    /// The wheel moves the highlight; a click highlights an entry, and a
    /// click on the highlighted entry runs it.
    fn handle_mouse_events(&mut self, mouse: MouseEvent) -> io::Result<Option<Action>> {
        let Some(layout) = self.last_layout else {
            return Ok(None);
        };
        match popup_list::mouse_hit(mouse, layout.popup, layout.offset) {
            MouseHit::Previous => self.move_selection(false),
            MouseHit::Next => self.move_selection(true),
            MouseHit::Outside => return Ok(Some(Action::HideLeaderMenu)),
            MouseHit::Line(line) => {
                let index = Self::entry_at(layout, line, mouse.column);
                if index == self.selected {
                    self.activate(index);
                } else if index < self.entries().len() {
                    self.selected = index;
                }
            }
            MouseHit::None => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect) -> io::Result<()> {
        if !self.visible {
            return Ok(());
        }
        let rows: Vec<PopupRow> = self
            .entries()
            .into_iter()
            .map(|entry| PopupRow {
                key: entry.key,
                label: vec![Span::raw(entry.description)],
                disabled: false,
            })
            .collect();
        if rows.is_empty() {
            return Ok(());
        }
        let theme = self.app_context.theme();
        let mut lines = popup_list::lines(theme, &rows, self.selected);
        let visible = usize::from(area.height.saturating_sub(2));
        // Too many for the height: side by side in two columns, when they fit
        let half = lines.len().div_ceil(2);
        let left_width = lines[..half].iter().map(Line::width).max().unwrap_or(0);
        let right_width = lines[half..].iter().map(Line::width).max().unwrap_or(0);
        let two_columns_fit = left_width + 2 + right_width + 4 <= usize::from(area.width);
        let mut columns = None;
        let selected_line = if lines.len() > visible && two_columns_fit {
            // The highlight is on the entry, not on the whole line
            for line in &mut lines {
                let style = std::mem::take(&mut line.style);
                for span in &mut line.spans {
                    span.style = style.patch(span.style);
                }
                // Pad the left entries so the right column lines up
                if line.width() < left_width {
                    line.spans
                        .push(Span::styled(" ".repeat(left_width - line.width()), style));
                }
            }
            let right = lines.split_off(half);
            for (line, right) in lines.iter_mut().zip(right) {
                line.spans.push(Span::raw("  "));
                line.spans.extend(right.spans);
            }
            columns = Some(half);
            self.selected % half
        } else {
            self.selected
        };
        let offset = popup_list::scroll_offset(selected_line, lines.len(), visible);
        let popup = popup_list::draw_lines(frame, area, theme, "Space", lines, offset, 28);
        // A border and a column of padding before the entries
        let right_x = popup.x + 2 + left_width as u16 + 2;
        self.last_layout = Some(Layout {
            popup,
            offset,
            columns: columns.map(|half| (half, right_x)),
        });
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/leader_menu.rs"]
mod tests;
