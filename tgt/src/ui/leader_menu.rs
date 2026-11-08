use crate::{
    app::action::Action,
    app::context::AppContext,
    settings::ActionBinding,
    ui::component_name::ComponentName,
    ui::component_traits::{Component, HandleFocus},
    ui::popup_list::{self, PopupRow},
};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
};
use std::{io, sync::Arc};
use tokio::sync::mpsc::UnboundedSender;

/// The menu opened by the leader key (Space), like concord's: it lists the
/// `[leader_menu]` keys, e.g. `1` to hide the chat list. The run loop runs
/// the pressed key's command and closes the menu.
pub struct LeaderMenu {
    app_context: Arc<AppContext>,
    name: String,
    action_tx: Option<UnboundedSender<Action>>,
    focused: bool,
    visible: bool,
}

impl LeaderMenu {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        LeaderMenu {
            app_context,
            name: String::new(),
            action_tx: None,
            focused: false,
            visible: false,
        }
    }

    pub fn with_name(mut self, name: impl AsRef<str>) -> Self {
        self.name = name.as_ref().to_string();
        self
    }

    /// `(key, description)` of every entry: digits first (the panes), then
    /// letters and symbols, then named keys like Space.
    pub fn entries(&self) -> Vec<(String, String)> {
        let Some(section) = self
            .app_context
            .keymap_config()
            .section(ComponentName::LeaderMenu)
        else {
            return Vec::new();
        };
        let mut entries: Vec<(String, String)> = section
            .iter()
            .filter_map(|(key, binding)| match binding {
                ActionBinding::Single {
                    action,
                    description,
                } => Some((
                    key.to_string(),
                    description.clone().unwrap_or_else(|| format!("{action:?}")),
                )),
                ActionBinding::Multiple(_) => None,
            })
            .collect();
        entries.sort_by(|(a, _), (b, _)| {
            let rank = |key: &str| match key.chars().next() {
                Some(c) if key.len() == 1 && c.is_ascii_digit() => 0,
                _ if key.chars().count() == 1 => 1,
                _ => 2,
            };
            (rank(a), a).cmp(&(rank(b), b))
        });
        entries
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
            Action::ShowLeaderMenu => self.visible = true,
            Action::HideLeaderMenu => self.visible = false,
            _ => {}
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect) -> io::Result<()> {
        if !self.visible {
            return Ok(());
        }
        let rows: Vec<PopupRow> = self
            .entries()
            .into_iter()
            .map(|(key, description)| PopupRow {
                key,
                label: vec![Span::raw(description)],
                disabled: false,
            })
            .collect();
        // No highlighted row: a key picks the entry
        let theme = self.app_context.theme();
        let mut lines = popup_list::lines(theme, &rows, usize::MAX);
        // Too many for the height: side by side in two columns, when they fit
        let half = lines.len().div_ceil(2);
        let left_width = lines[..half].iter().map(Line::width).max().unwrap_or(0);
        let right_width = lines[half..].iter().map(Line::width).max().unwrap_or(0);
        let two_columns_fit = left_width + 2 + right_width + 4 <= usize::from(area.width);
        if lines.len() + 2 > usize::from(area.height) && two_columns_fit {
            let right = lines.split_off(half);
            for (line, right) in lines.iter_mut().zip(right) {
                let gap = left_width - line.width() + 2;
                line.spans.push(Span::raw(" ".repeat(gap)));
                line.spans.extend(right.spans);
            }
        }
        popup_list::draw_lines(frame, area, theme, "Space", lines, 0, 28);
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/leader_menu.rs"]
mod tests;
