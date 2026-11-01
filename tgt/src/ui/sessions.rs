//! Popup listing your active sessions, to log other devices out or confirm
//! a new sign-in as yours.

use crate::{
    app::action::Action,
    app::context::AppContext,
    ui::component_name::ComponentName,
    ui::component_traits::{Component, HandleFocus},
    ui::popup_list::{self, PopupRow},
};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use libtgt::model::SessionEntry;
use ratatui::{Frame, layout::Rect, text::Span};
use std::{io, sync::Arc};
use tokio::sync::mpsc::UnboundedSender;

/// Said when trying to end this app's own session here.
pub const CURRENT_SESSION: &str = "This is tgt's own session: `tgt logout` ends it.";

/// What a second press of the same key will end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Armed {
    One(i64),
    AllOthers,
}

pub struct Sessions {
    app_context: Arc<AppContext>,
    name: String,
    action_tx: Option<UnboundedSender<Action>>,
    focused: bool,
    visible: bool,
    /// None while loading.
    entries: Option<Vec<SessionEntry>>,
    selected: usize,
    armed: Option<Armed>,
    /// Popup bounds and first visible row from the most recent draw.
    last_popup: Option<(Rect, usize)>,
}

impl Sessions {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Sessions {
            app_context,
            name: String::new(),
            action_tx: None,
            focused: false,
            visible: false,
            entries: None,
            selected: 0,
            armed: None,
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

    fn len(&self) -> usize {
        self.entries.as_ref().map_or(0, Vec::len)
    }

    fn selected_entry(&self) -> Option<&SessionEntry> {
        self.entries.as_ref()?.get(self.selected)
    }

    fn move_selection(&mut self, forward: bool) {
        let len = self.len();
        if len > 0 {
            self.armed = None;
            self.selected = if forward {
                (self.selected + 1) % len
            } else {
                (self.selected + len - 1) % len
            };
        }
    }

    /// Ends the highlighted session on the second press; it leaves the list
    /// right away.
    fn terminate(&mut self) {
        let Some(entry) = self.selected_entry().cloned() else {
            return;
        };
        if entry.is_current {
            self.armed = None;
            self.send(Action::StatusMessage(CURRENT_SESSION.into()));
            return;
        }
        if self.armed != Some(Armed::One(entry.id)) {
            self.armed = Some(Armed::One(entry.id));
            return;
        }
        self.armed = None;
        if let Some(entries) = self.entries.as_mut() {
            entries.remove(self.selected);
            self.selected = self.selected.min(entries.len().saturating_sub(1));
        }
        self.send(Action::TerminateSession(entry.id, entry.application));
    }

    /// Ends every session but this app's on the second press.
    fn terminate_others(&mut self) {
        let Some(entries) = self.entries.as_ref() else {
            return;
        };
        if entries.iter().all(|entry| entry.is_current) {
            self.armed = None;
            self.send(Action::StatusMessage("No other sessions.".into()));
            return;
        }
        if self.armed != Some(Armed::AllOthers) {
            self.armed = Some(Armed::AllOthers);
            return;
        }
        self.armed = None;
        if let Some(entries) = self.entries.as_mut() {
            entries.retain(|entry| entry.is_current);
        }
        self.selected = 0;
        self.send(Action::TerminateOtherSessions);
    }

    /// Confirms the highlighted new sign-in as yours.
    fn confirm(&mut self) {
        self.armed = None;
        let Some(entry) = self.entries.as_mut().and_then(|e| e.get_mut(self.selected)) else {
            return;
        };
        if !entry.is_unconfirmed {
            self.send(Action::StatusMessage(
                "Only new sign-ins need confirming.".into(),
            ));
            return;
        }
        entry.is_unconfirmed = false;
        let action = Action::ConfirmSession(entry.id, entry.application.clone());
        self.send(action);
    }

    /// The shortest key bound to `action` in this popup.
    fn key_of(&self, action: Action) -> Option<String> {
        let mut keys = self
            .app_context
            .keymap_config()
            .get_key_of_single_action(ComponentName::Sessions, action);
        keys.sort_by_key(|key| key.to_string().len());
        keys.first().map(ToString::to_string)
    }

    fn title(&self) -> String {
        let count = match self.entries.as_ref() {
            Some(entries) if !entries.is_empty() => format!(" ({})", entries.len()),
            _ => String::new(),
        };
        let terminate = self.key_of(Action::SessionsTerminate);
        let others = self.key_of(Action::SessionsTerminateOthers);
        match self.armed {
            Some(Armed::One(_)) => match terminate {
                Some(key) => format!("Press {key} again to log it out"),
                None => "Press again to log it out".to_owned(),
            },
            Some(Armed::AllOthers) => match others {
                Some(key) => format!("Press {key} again to log out all others"),
                None => "Press again to log out all others".to_owned(),
            },
            None if self.len() > 1 => {
                let mut title = format!("Sessions{count}");
                if let Some(key) = terminate {
                    title.push_str(&format!(" · {key} logs out"));
                }
                if let Some(key) = others {
                    title.push_str(&format!(" · {key} all others"));
                }
                title
            }
            None => format!("Sessions{count}"),
        }
    }

    #[cfg(test)]
    fn ids(&self) -> Vec<i64> {
        self.entries
            .iter()
            .flatten()
            .map(|entry| entry.id)
            .collect()
    }
}

impl HandleFocus for Sessions {
    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl Component for Sessions {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> io::Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn update(&mut self, action: Action) {
        match action {
            Action::ShowSessions => {
                self.visible = true;
                self.entries = None;
                self.selected = 0;
                self.armed = None;
            }
            Action::HideSessions => {
                self.visible = false;
                self.entries = None;
                self.armed = None;
            }
            Action::SessionsLoaded(entries) if self.visible => {
                self.selected = self.selected.min(entries.len().saturating_sub(1));
                self.entries = Some(entries);
                self.armed = None;
            }
            Action::SessionsNext => self.move_selection(true),
            Action::SessionsPrevious => self.move_selection(false),
            Action::SessionsTerminate => self.terminate(),
            Action::SessionsTerminateOthers => self.terminate_others(),
            Action::SessionsConfirm => self.confirm(),
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
                    return Ok(Some(Action::HideSessions));
                }
                if mouse.row > popup.y && mouse.row + 1 < popup.bottom() {
                    let index = offset + usize::from(mouse.row - popup.y - 1);
                    if index < self.len() && index != self.selected {
                        self.selected = index;
                        self.armed = None;
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
        let theme = self.app_context.theme();
        let placeholder = |text: &'static str| PopupRow {
            key: String::new(),
            label: vec![Span::raw(text)],
            disabled: true,
        };
        let rows: Vec<PopupRow> = match self.entries.as_ref() {
            None => vec![placeholder("Loading…")],
            Some(entries) if entries.is_empty() => vec![placeholder("No sessions")],
            Some(entries) => entries
                .iter()
                .map(|entry| PopupRow {
                    key: String::new(),
                    label: vec![Span::raw(format!(
                        "{} {}",
                        entry.device_kind.icon(),
                        entry.summary()
                    ))],
                    disabled: false,
                })
                .collect(),
        };
        let selected = if self.len() > 0 {
            self.selected
        } else {
            usize::MAX
        };
        self.last_popup = Some(popup_list::draw(
            frame,
            area,
            theme,
            &self.title(),
            &rows,
            selected,
            48,
        ));
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/sessions.rs"]
mod tests;
