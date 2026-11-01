//! Popup with your privacy settings: who sees your last seen time, phone
//! number, profile photo and more, with the person of the open private
//! chat as an exception, and whether others see when you read messages.

use crate::{
    app::action::Action,
    app::context::AppContext,
    ui::component_name::ComponentName,
    ui::component_traits::{Component, HandleFocus},
    ui::popup_list::{self, PopupRow},
};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use libtgt::model::{Exception, PrivacySetting, PrivacySettings};
use ratatui::{Frame, layout::Rect, text::Span};
use std::{io, sync::Arc};
use tokio::sync::mpsc::UnboundedSender;

/// Width of the setting labels, to line the values up.
const LABEL_WIDTH: usize = 20;
/// The row after the settings.
const READ_TIME: &str = "Read time";

pub struct Privacy {
    app_context: Arc<AppContext>,
    name: String,
    action_tx: Option<UnboundedSender<Action>>,
    focused: bool,
    visible: bool,
    /// None while loading.
    settings: Option<PrivacySettings>,
    /// The other party of the open private chat, to make an exception:
    /// their id and first name.
    person: Option<(i64, String)>,
    /// Index of the highlighted row: a setting, or the read time last.
    selected: usize,
    /// Popup bounds and first visible row from the most recent draw.
    last_popup: Option<(Rect, usize)>,
}

impl Privacy {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Privacy {
            app_context,
            name: String::new(),
            action_tx: None,
            focused: false,
            visible: false,
            settings: None,
            person: None,
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

    /// The open chat's person, unless it's your own chat.
    fn open_chat_person(&self) -> Option<(i64, String)> {
        let store = self.app_context.store();
        let user = store.chat_user(store.open_chat_id_i64())?;
        (user.id != store.me()).then_some((user.id, user.first_name))
    }

    fn len(&self) -> usize {
        self.settings.as_ref().map_or(0, |s| s.rules.len() + 1)
    }

    /// The highlighted setting; None on the read time.
    fn selected_setting(&self) -> Option<PrivacySetting> {
        self.settings
            .as_ref()?
            .rules
            .get(self.selected)
            .map(|(setting, _)| *setting)
    }

    fn move_selection(&mut self, forward: bool) {
        let len = self.len();
        if len > 0 {
            self.selected = if forward {
                (self.selected + 1) % len
            } else {
                (self.selected + len - 1) % len
            };
        }
    }

    /// Moves the highlighted setting to its next audience, or turns the
    /// read time on or off.
    fn cycle(&mut self) {
        let setting = self.selected_setting();
        let Some(settings) = self.settings.as_mut() else {
            return;
        };
        let action = match setting {
            Some(setting) => {
                let Some(rules) = settings.rules_mut(setting) else {
                    return;
                };
                rules.audience = rules.audience.next(setting.audiences());
                Action::SetPrivacyRules(setting, rules.clone())
            }
            None => {
                settings.show_read_date = !settings.show_read_date;
                Action::SetShowReadDate(settings.show_read_date)
            }
        };
        self.send(action);
    }

    /// Makes the open chat's person an exception to the highlighted setting.
    fn set_exception(&mut self, exception: Option<Exception>) {
        let Some((user_id, name)) = self.person.clone() else {
            self.send(Action::StatusMessage(
                "Open a private chat to make its person an exception.".into(),
            ));
            return;
        };
        let Some(setting) = self.selected_setting() else {
            self.send(Action::StatusMessage(
                "The read time is the same for everyone.".into(),
            ));
            return;
        };
        if !setting.has_exceptions() {
            self.send(Action::StatusMessage(format!(
                "{} has no exceptions.",
                setting.label()
            )));
            return;
        }
        let Some(rules) = self
            .settings
            .as_mut()
            .and_then(|settings| settings.rules_mut(setting))
        else {
            return;
        };
        if rules.exception(user_id) == exception {
            let already = match exception {
                Some(Exception::Always) => "always shares",
                Some(Exception::Never) => "never shares",
                None => "follows the audience",
            };
            self.send(Action::StatusMessage(format!(
                "{} {already} with {name}.",
                setting.label()
            )));
            return;
        }
        rules.set_exception(user_id, exception);
        let action = Action::SetPrivacyRules(setting, rules.clone());
        self.send(action);
    }

    /// The shortest key bound to `action` in this popup.
    fn key_of(&self, action: Action) -> Option<String> {
        let mut keys = self
            .app_context
            .keymap_config()
            .get_key_of_single_action(ComponentName::Privacy, action);
        keys.sort_by_key(|key| key.to_string().len());
        keys.first().map(ToString::to_string)
    }

    fn title(&self) -> String {
        let mut title = "Privacy".to_owned();
        if let Some(key) = self.key_of(Action::PrivacyCycle) {
            title.push_str(&format!(" · {key} changes"));
        }
        if let Some((_, name)) = &self.person {
            let keys: Vec<String> = [
                Action::PrivacyAlways,
                Action::PrivacyNever,
                Action::PrivacyFollow,
            ]
            .into_iter()
            .filter_map(|action| self.key_of(action))
            .collect();
            if keys.len() == 3 {
                title.push_str(&format!(
                    " · {}/{}/{} {name} always/never/default",
                    keys[0], keys[1], keys[2]
                ));
            }
        }
        title
    }

    /// The row texts, label and value.
    fn texts(&self) -> Vec<(String, String)> {
        let Some(settings) = self.settings.as_ref() else {
            return Vec::new();
        };
        let mut texts: Vec<(String, String)> = settings
            .rules
            .iter()
            .map(|(setting, rules)| {
                let mut value = rules.summary();
                if let Some((user_id, name)) = &self.person {
                    match rules.exception(*user_id) {
                        Some(Exception::Always) => value.push_str(&format!(" · {name}: always")),
                        Some(Exception::Never) => value.push_str(&format!(" · {name}: never")),
                        None => {}
                    }
                }
                (setting.label().to_owned(), value)
            })
            .collect();
        let read = if settings.show_read_date {
            "Shown"
        } else {
            "Hidden (and others' from you)"
        };
        texts.push((READ_TIME.to_owned(), read.to_owned()));
        texts
    }

    #[cfg(test)]
    fn shown(&self) -> Vec<String> {
        self.texts()
            .into_iter()
            .map(|(label, value)| format!("{label}: {value}"))
            .collect()
    }

    #[cfg(test)]
    fn set_person(&mut self, user_id: i64, name: &str) {
        self.person = Some((user_id, name.to_owned()));
    }
}

impl HandleFocus for Privacy {
    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl Component for Privacy {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> io::Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn update(&mut self, action: Action) {
        match action {
            Action::ShowPrivacy => {
                self.visible = true;
                self.settings = None;
                self.selected = 0;
                self.person = self.open_chat_person();
            }
            Action::HidePrivacy => {
                self.visible = false;
                self.settings = None;
            }
            Action::PrivacyLoaded(settings) if self.visible => {
                self.selected = self.selected.min(settings.rules.len());
                self.settings = Some(settings);
            }
            Action::PrivacyNext => self.move_selection(true),
            Action::PrivacyPrevious => self.move_selection(false),
            Action::PrivacyCycle => self.cycle(),
            Action::PrivacyAlways => self.set_exception(Some(Exception::Always)),
            Action::PrivacyNever => self.set_exception(Some(Exception::Never)),
            Action::PrivacyFollow => self.set_exception(None),
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
                    return Ok(Some(Action::HidePrivacy));
                }
                if mouse.row > popup.y && mouse.row + 1 < popup.bottom() {
                    let index = offset + usize::from(mouse.row - popup.y - 1);
                    if index < self.len() {
                        if index == self.selected {
                            self.cycle();
                        } else {
                            self.selected = index;
                        }
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
        let rows: Vec<PopupRow> = if self.settings.is_none() {
            vec![PopupRow {
                key: String::new(),
                label: vec![Span::raw("Loading…")],
                disabled: true,
            }]
        } else {
            self.texts()
                .into_iter()
                .map(|(label, value)| PopupRow {
                    key: String::new(),
                    label: vec![Span::raw(format!("{label:LABEL_WIDTH$}")), Span::raw(value)],
                    disabled: false,
                })
                .collect()
        };
        let selected = if self.settings.is_some() {
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
            56,
        ));
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/privacy.rs"]
mod tests;
