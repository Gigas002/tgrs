//! Popup with Telegram's notification settings: the open chat's own ones,
//! and the defaults of private chats, groups and channels. They're kept by
//! Telegram, so they apply on every device.

use crate::{
    app::action::Action,
    app::context::AppContext,
    settings::HighlightGroup,
    ui::component_traits::{Component, HandleFocus},
    ui::popup_list::{self, PopupRow},
};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use libtgrs::model::{NotificationOption, NotificationSettings, NotificationTarget};
use ratatui::{Frame, layout::Rect, text::Span};
use std::{io, sync::Arc};
use tokio::sync::mpsc::UnboundedSender;

const TITLE: &str = "Notifications · enter toggles · saved in Telegram for every device";
/// Width of the option labels, to line the values up.
const LABEL_WIDTH: usize = 22;

/// One line of the popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Row {
    /// The open chat's title, or a scope's name, above its options.
    Header(Option<libtgrs::model::NotificationScope>),
    Option(NotificationTarget, NotificationOption),
}

/// What a chat's option goes to next: its default, then the opposite of
/// the default, then the default set explicitly.
fn next_chat_value(current: Option<bool>, default: bool) -> Option<bool> {
    match current {
        None => Some(!default),
        Some(value) if value != default => Some(default),
        Some(_) => None,
    }
}

fn on_off(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

pub struct NotificationSettingsPopup {
    app_context: Arc<AppContext>,
    name: String,
    action_tx: Option<UnboundedSender<Action>>,
    focused: bool,
    visible: bool,
    /// None while loading.
    settings: Option<NotificationSettings>,
    rows: Vec<Row>,
    /// Index into `rows`, always on an option.
    selected: usize,
    /// Popup bounds and first visible row from the most recent draw.
    last_popup: Option<(Rect, usize)>,
}

impl NotificationSettingsPopup {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        NotificationSettingsPopup {
            app_context,
            name: String::new(),
            action_tx: None,
            focused: false,
            visible: false,
            settings: None,
            rows: Vec::new(),
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

    /// The open chat's options first, then each scope's.
    fn build_rows(settings: &NotificationSettings) -> Vec<Row> {
        let mut rows = Vec::new();
        if let Some(chat) = &settings.chat {
            rows.push(Row::Header(None));
            rows.extend(
                chat.scope
                    .options()
                    .iter()
                    .map(|option| Row::Option(NotificationTarget::Chat(chat.chat_id), *option)),
            );
        }
        for scope in &settings.scopes {
            rows.push(Row::Header(Some(scope.scope)));
            rows.extend(
                scope
                    .scope
                    .options()
                    .iter()
                    .map(|option| Row::Option(NotificationTarget::Scope(scope.scope), *option)),
            );
        }
        rows
    }

    fn is_option(&self, index: usize) -> bool {
        matches!(self.rows.get(index), Some(Row::Option(..)))
    }

    fn first_option(&self) -> usize {
        (0..self.rows.len())
            .find(|index| self.is_option(*index))
            .unwrap_or(0)
    }

    fn move_selection(&mut self, forward: bool) {
        let len = self.rows.len();
        if len == 0 {
            return;
        }
        let mut index = self.selected;
        for _ in 0..len {
            index = if forward {
                (index + 1) % len
            } else {
                (index + len - 1) % len
            };
            if self.is_option(index) {
                self.selected = index;
                return;
            }
        }
    }

    /// Changes the highlighted option right away and saves it in Telegram.
    fn toggle(&mut self) {
        let Some(Row::Option(target, option)) = self.rows.get(self.selected).copied() else {
            return;
        };
        let Some(settings) = self.settings.as_mut() else {
            return;
        };
        let value = match target {
            NotificationTarget::Scope(scope) => {
                let Some(scope) = settings.scopes.iter_mut().find(|s| s.scope == scope) else {
                    return;
                };
                let on = !scope.get(option);
                scope.set(option, on);
                Some(on)
            }
            NotificationTarget::Chat(_) => {
                let Some(chat) = settings.chat.as_ref() else {
                    return;
                };
                let default = settings
                    .scope(chat.scope)
                    .is_none_or(|scope| scope.get(option));
                let value = next_chat_value(chat.get(option), default);
                if let Some(chat) = settings.chat.as_mut() {
                    chat.set(option, value);
                }
                value
            }
        };
        self.send(Action::SetNotificationOption(target, option, value));
    }

    fn header_text(&self, scope: Option<libtgrs::model::NotificationScope>) -> String {
        match scope {
            Some(scope) => scope.label().to_owned(),
            None => match self.settings.as_ref().and_then(|s| s.chat.as_ref()) {
                Some(chat) if !chat.title.is_empty() => format!("This chat · {}", chat.title),
                _ => "This chat".to_owned(),
            },
        }
    }

    /// The value shown for an option, e.g. `on` or `default (off)`.
    fn value_text(&self, target: NotificationTarget, option: NotificationOption) -> String {
        let Some(settings) = self.settings.as_ref() else {
            return String::new();
        };
        match target {
            NotificationTarget::Scope(scope) => settings
                .scope(scope)
                .map_or("?", |scope| on_off(scope.get(option)))
                .to_owned(),
            NotificationTarget::Chat(_) => {
                let Some(chat) = settings.chat.as_ref() else {
                    return String::new();
                };
                match chat.get(option) {
                    Some(on) => on_off(on).to_owned(),
                    None => format!("default ({})", on_off(settings.effective(chat, option))),
                }
            }
        }
    }

    #[cfg(test)]
    fn shown(&self) -> Vec<String> {
        self.rows
            .iter()
            .map(|row| match row {
                Row::Header(scope) => self.header_text(*scope),
                Row::Option(target, option) => {
                    format!("{}: {}", option.label(), self.value_text(*target, *option))
                }
            })
            .collect()
    }
}

impl HandleFocus for NotificationSettingsPopup {
    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl Component for NotificationSettingsPopup {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> io::Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn update(&mut self, action: Action) {
        match action {
            Action::ShowNotificationSettings => {
                self.visible = true;
                self.settings = None;
                self.rows.clear();
                self.selected = 0;
            }
            Action::HideNotificationSettings => {
                self.visible = false;
                self.settings = None;
                self.rows.clear();
            }
            Action::NotificationSettingsLoaded(settings) if self.visible => {
                let rows = Self::build_rows(&settings);
                // Keep the same option highlighted when reloaded
                let keep = self.rows.get(self.selected).copied();
                self.rows = rows;
                self.settings = Some(settings);
                self.selected = keep
                    .and_then(|row| self.rows.iter().position(|r| *r == row))
                    .unwrap_or_else(|| self.first_option());
            }
            Action::NotificationSettingsNext => self.move_selection(true),
            Action::NotificationSettingsPrevious => self.move_selection(false),
            Action::NotificationSettingsToggle => self.toggle(),
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
                    return Ok(Some(Action::HideNotificationSettings));
                }
                if mouse.row > popup.y && mouse.row + 1 < popup.bottom() {
                    let index = offset + usize::from(mouse.row - popup.y - 1);
                    if self.is_option(index) {
                        if index == self.selected {
                            self.toggle();
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
            self.rows
                .iter()
                .map(|row| match row {
                    Row::Header(scope) => PopupRow {
                        key: String::new(),
                        label: vec![Span::styled(
                            self.header_text(*scope),
                            theme.style(HighlightGroup::ModalTitle),
                        )],
                        disabled: false,
                    },
                    Row::Option(target, option) => PopupRow {
                        key: String::new(),
                        label: vec![
                            Span::raw(format!("  {:LABEL_WIDTH$}", option.label())),
                            Span::raw(self.value_text(*target, *option)),
                        ],
                        disabled: false,
                    },
                })
                .collect()
        };
        let selected = if self.is_option(self.selected) {
            self.selected
        } else {
            usize::MAX
        };
        self.last_popup = Some(popup_list::draw(
            frame, area, theme, TITLE, &rows, selected, 56,
        ));
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/notification_settings.rs"]
mod tests;
