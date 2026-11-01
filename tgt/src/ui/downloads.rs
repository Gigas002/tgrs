//! Popup listing the downloads started from messages, with their progress,
//! to pause, resume, cancel or open them.

use crate::{
    app::action::Action,
    app::context::AppContext,
    ui::component_traits::{Component, HandleFocus},
    ui::popup_list::{self, PopupRow},
};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use libtgt::model::{Transfer, TransferPurpose, TransferState};
use ratatui::{Frame, layout::Rect, text::Span};
use std::{io, sync::Arc};
use tokio::sync::mpsc::UnboundedSender;

const TITLE: &str = "Downloads · p pause/resume · x cancel · enter open · c clear";

/// What a download's row starts with.
fn state_icon(state: &TransferState) -> &'static str {
    match state {
        TransferState::Downloading => "⬇",
        TransferState::Paused => "⏸",
        TransferState::Done => "✓",
        TransferState::Cancelled | TransferState::Failed(_) => "✗",
    }
}

pub struct Downloads {
    app_context: Arc<AppContext>,
    name: String,
    action_tx: Option<UnboundedSender<Action>>,
    focused: bool,
    visible: bool,
    /// The downloads as last read from the store, latest first.
    entries: Vec<Transfer>,
    selected: usize,
    /// Popup bounds and first visible row from the most recent draw.
    last_popup: Option<(Rect, usize)>,
}

impl Downloads {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Downloads {
            app_context,
            name: String::new(),
            action_tx: None,
            focused: false,
            visible: false,
            entries: Vec::new(),
            selected: 0,
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

    /// Reads the downloads again, keeping the same one selected.
    fn refresh(&mut self) {
        let selected_id = self.entries.get(self.selected).map(|t| t.file_id);
        self.entries = self.app_context.store().transfers();
        self.selected = selected_id
            .and_then(|id| self.entries.iter().position(|t| t.file_id == id))
            .unwrap_or(0)
            .min(self.entries.len().saturating_sub(1));
    }

    fn move_selection(&mut self, forward: bool) {
        let len = self.entries.len();
        if len > 0 {
            self.selected = if forward {
                (self.selected + 1) % len
            } else {
                (self.selected + len - 1) % len
            };
        }
    }

    fn selected(&self) -> Option<&Transfer> {
        self.entries.get(self.selected)
    }

    fn toggle_pause(&mut self) {
        let Some(transfer) = self.selected() else {
            return;
        };
        let action = match transfer.state {
            TransferState::Downloading => Action::PauseDownload(transfer.file_id),
            TransferState::Paused | TransferState::Failed(_) | TransferState::Cancelled => {
                Action::FetchMedia(transfer.file_id, transfer.purpose)
            }
            TransferState::Done => {
                Action::StatusMessage(format!("{} is downloaded already.", transfer.name))
            }
        };
        self.send(action);
    }

    fn cancel(&mut self) {
        let Some(transfer) = self.selected() else {
            return;
        };
        let action = match transfer.state {
            TransferState::Downloading | TransferState::Paused => {
                Action::CancelDownload(transfer.file_id)
            }
            _ => Action::StatusMessage(format!("{} isn't downloading.", transfer.name)),
        };
        self.send(action);
    }

    fn open(&mut self) {
        let Some(transfer) = self.selected() else {
            return;
        };
        let action = match transfer.state {
            TransferState::Done => Action::FetchMedia(transfer.file_id, TransferPurpose::Open),
            _ => Action::StatusMessage(format!("{} isn't downloaded yet.", transfer.name)),
        };
        self.send(action);
    }

    fn clear_finished(&mut self) {
        self.app_context.store().clear_finished_transfers();
        self.refresh();
    }

    #[cfg(test)]
    fn names(&self) -> Vec<&str> {
        self.entries.iter().map(|t| t.name.as_str()).collect()
    }
}

impl HandleFocus for Downloads {
    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl Component for Downloads {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> io::Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn update(&mut self, action: Action) {
        match action {
            Action::ShowDownloads => {
                self.visible = true;
                self.entries.clear();
                self.selected = 0;
                self.refresh();
            }
            Action::HideDownloads => self.visible = false,
            Action::DownloadsNext => self.move_selection(true),
            Action::DownloadsPrevious => self.move_selection(false),
            Action::DownloadsTogglePause => self.toggle_pause(),
            Action::DownloadsCancel => self.cancel(),
            Action::DownloadsOpen => self.open(),
            Action::DownloadsClearFinished => self.clear_finished(),
            _ => {}
        }
    }

    fn handle_mouse_events(&mut self, mouse: MouseEvent) -> io::Result<Option<Action>> {
        let Some((popup, offset)) = self.last_popup else {
            return Ok(None);
        };
        match mouse.kind {
            MouseEventKind::ScrollUp => self.move_selection(false),
            MouseEventKind::ScrollDown => self.move_selection(true),
            MouseEventKind::Down(MouseButton::Left) => {
                let inside = mouse.column >= popup.x
                    && mouse.column < popup.right()
                    && mouse.row >= popup.y
                    && mouse.row < popup.bottom();
                if !inside {
                    return Ok(Some(Action::HideDownloads));
                }
                if mouse.row > popup.y && mouse.row + 1 < popup.bottom() {
                    let index = offset + usize::from(mouse.row - popup.y - 1);
                    if index < self.entries.len() {
                        self.selected = index;
                    }
                }
            }
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect) -> io::Result<()> {
        if !self.visible {
            return Ok(());
        }
        // Progress comes in from TDLib all the time
        self.refresh();
        let theme = self.app_context.theme();
        let rows: Vec<PopupRow> = if self.entries.is_empty() {
            vec![PopupRow {
                key: String::new(),
                label: vec![Span::raw(
                    "No downloads yet: s or x on a message starts one",
                )],
                disabled: true,
            }]
        } else {
            self.entries
                .iter()
                .map(|transfer| PopupRow {
                    key: String::new(),
                    label: vec![Span::raw(format!(
                        "{} {} · {}",
                        state_icon(&transfer.state),
                        transfer.name,
                        transfer.status_text()
                    ))],
                    disabled: transfer.is_finished() && transfer.state != TransferState::Done,
                })
                .collect()
        };
        let selected = if self.entries.is_empty() {
            usize::MAX
        } else {
            self.selected
        };
        self.last_popup = Some(popup_list::draw(
            frame, area, theme, TITLE, &rows, selected, 56,
        ));
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/downloads.rs"]
mod tests;
