use crate::{
    app::action::{Action, PickerKind},
    app::context::AppContext,
    settings::HighlightGroup,
    ui::component_traits::{Component, HandleFocus},
    ui::popup_list::{self, PopupRow},
};
use crossterm::event::KeyCode;
use ratatui::{Frame, layout::Rect, text::Span};
use std::{io, sync::Arc};
use tokio::sync::mpsc::UnboundedSender;

/// The key that picks the entry at `index` directly: 1-9, then 0.
pub fn indexed_shortcut(index: usize) -> Option<char> {
    match index {
        0..=8 => char::from_digit(u32::try_from(index + 1).ok()?, 10),
        9 => Some('0'),
        _ => None,
    }
}

/// Popup to pick a reaction to toggle on a message, or a link to open.
pub struct Picker {
    app_context: Arc<AppContext>,
    name: String,
    action_tx: Option<UnboundedSender<Action>>,
    focused: bool,
    kind: Option<PickerKind>,
    selected: usize,
}

impl Picker {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Picker {
            app_context,
            name: String::new(),
            action_tx: None,
            focused: false,
            kind: None,
            selected: 0,
        }
    }

    pub fn with_name(mut self, name: impl AsRef<str>) -> Self {
        self.name = name.as_ref().to_string();
        self
    }

    fn len(&self) -> usize {
        match &self.kind {
            Some(PickerKind::Reactions { reactions, .. }) => reactions.len(),
            Some(PickerKind::Links(links)) => links.len(),
            None => 0,
        }
    }

    fn move_selection(&mut self, forward: bool) {
        let len = self.len();
        if len == 0 {
            return;
        }
        self.selected = if forward {
            (self.selected + 1) % len
        } else {
            (self.selected + len - 1) % len
        };
    }

    /// Closes the picker and acts on the entry at `index`.
    fn pick(&mut self, index: usize) {
        let action = match &self.kind {
            Some(PickerKind::Reactions {
                message_id,
                reactions,
            }) => reactions.get(index).map(|reaction| {
                Action::SetReaction(*message_id, reaction.emoji.clone(), !reaction.chosen)
            }),
            Some(PickerKind::Links(links)) => links.get(index).cloned().map(Action::OpenUrl),
            None => None,
        };
        let (Some(action), Some(tx)) = (action, self.action_tx.as_ref()) else {
            return;
        };
        let _ = tx.send(Action::HidePicker);
        let _ = tx.send(action);
    }

    fn title(&self) -> &'static str {
        match self.kind {
            Some(PickerKind::Reactions { .. }) => "Choose reaction",
            Some(PickerKind::Links(_)) => "Open URL",
            None => "",
        }
    }

    fn rows(&self) -> Vec<PopupRow> {
        let theme = self.app_context.theme();
        let key = |index| {
            indexed_shortcut(index)
                .map(String::from)
                .unwrap_or_default()
        };
        match &self.kind {
            Some(PickerKind::Reactions { reactions, .. }) => reactions
                .iter()
                .enumerate()
                .map(|(index, reaction)| {
                    let mut label = vec![Span::styled(
                        reaction.emoji.clone(),
                        theme.style(if reaction.chosen {
                            HighlightGroup::SelfReaction
                        } else {
                            HighlightGroup::Reaction
                        }),
                    )];
                    if reaction.chosen {
                        label.push(Span::styled("  remove", theme.style(HighlightGroup::Muted)));
                    }
                    PopupRow {
                        key: key(index),
                        label,
                        disabled: false,
                    }
                })
                .collect(),
            Some(PickerKind::Links(links)) => links
                .iter()
                .enumerate()
                .map(|(index, link)| PopupRow {
                    key: key(index),
                    label: vec![Span::styled(
                        link.clone(),
                        theme.style(HighlightGroup::MessageLink),
                    )],
                    disabled: false,
                })
                .collect(),
            None => Vec::new(),
        }
    }

    #[cfg(test)]
    pub fn kind(&self) -> Option<&PickerKind> {
        self.kind.as_ref()
    }
}

impl HandleFocus for Picker {
    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl Component for Picker {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> io::Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn update(&mut self, action: Action) {
        match action {
            Action::ShowPicker(kind) => {
                self.kind = Some(kind);
                self.selected = 0;
            }
            Action::HidePicker => self.kind = None,
            Action::PickerPrevious => self.move_selection(false),
            Action::PickerNext => self.move_selection(true),
            Action::PickerConfirm => self.pick(self.selected),
            Action::Key(KeyCode::Char(c), _) if self.kind.is_some() => {
                if let Some(index) = (0..self.len()).find(|&i| indexed_shortcut(i) == Some(c)) {
                    self.selected = index;
                    self.pick(index);
                }
            }
            _ => {}
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect) -> io::Result<()> {
        if self.kind.is_none() {
            return Ok(());
        }
        let rows = self.rows();
        let min_width = match self.kind {
            Some(PickerKind::Links(_)) => 54,
            _ => 28,
        };
        popup_list::draw(
            frame,
            area,
            self.app_context.theme(),
            self.title(),
            &rows,
            self.selected,
            min_width,
        );
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/picker.rs"]
mod tests;
