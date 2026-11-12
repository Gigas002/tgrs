//! Asks whether images are sent as photos (compressed by Telegram) or as
//! files (untouched), like Telegram's own apps do.

use crate::{
    app::action::Action,
    app::context::AppContext,
    ui::component_name::ComponentName,
    ui::component_traits::{Component, HandleFocus},
    ui::popup_list::{self, MouseHit, PopupRow},
};
use crossterm::event::MouseEvent;
use libtgrs::client::SendAs;
use ratatui::{Frame, layout::Rect, text::Span};
use std::{io, sync::Arc};
use tokio::sync::mpsc::UnboundedSender;

/// The choices, in the order shown.
const CHOICES: [(SendAs, &str); 2] = [
    (SendAs::Photos, "as photos (compressed)"),
    (SendAs::Files, "as files (original quality)"),
];

/// Popup asking how to send the images about to be sent.
pub struct SendAsPopup {
    app_context: Arc<AppContext>,
    name: String,
    action_tx: Option<UnboundedSender<Action>>,
    focused: bool,
    visible: bool,
    /// The images among the files being sent.
    images: usize,
    /// Files that aren't images, sent as files either way.
    others: usize,
    selected: usize,
    /// Where the popup was drawn and its scroll offset, for mouse clicks.
    last_popup: Option<(Rect, usize)>,
}

impl SendAsPopup {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        SendAsPopup {
            app_context,
            name: String::new(),
            action_tx: None,
            focused: false,
            visible: false,
            images: 0,
            others: 0,
            selected: 0,
            last_popup: None,
        }
    }

    pub fn with_name(mut self, name: impl AsRef<str>) -> Self {
        self.name = name.as_ref().to_string();
        self
    }

    /// The popup's title, e.g. `Send 2 images as` or, with other files,
    /// `Send 2 images (and 1 other file) as`.
    fn title(&self) -> String {
        let plural = |n: usize, one: &str, many: &str| {
            if n == 1 {
                format!("{n} {one}")
            } else {
                format!("{n} {many}")
            }
        };
        let images = if self.images == 1 && self.others == 0 {
            "the image".to_owned()
        } else {
            plural(self.images, "image", "images")
        };
        match self.others {
            0 => format!("Send {images} as"),
            n => format!(
                "Send {images} (and {}) as",
                plural(n, "other file", "other files")
            ),
        }
    }

    /// The shortest key of the popup bound to sending `send_as`, e.g. `p`.
    fn key_label(&self, send_as: SendAs) -> String {
        self.app_context
            .keymap_config()
            .get_key_of_single_action(ComponentName::SendAs, Action::SendAsChosen(send_as))
            .iter()
            .map(ToString::to_string)
            .min_by_key(|key| (key.chars().count(), key.clone()))
            .unwrap_or_default()
    }

    fn move_selection(&mut self, forward: bool) {
        let len = CHOICES.len();
        self.selected = if forward {
            (self.selected + 1) % len
        } else {
            (self.selected + len - 1) % len
        };
    }

    /// Sends the files the way at `index`.
    fn choose(&mut self, index: usize) {
        let Some((send_as, ..)) = CHOICES.get(index) else {
            return;
        };
        if let Some(tx) = self.action_tx.as_ref() {
            let _ = tx.send(Action::SendAsChosen(*send_as));
        }
    }
}

impl HandleFocus for SendAsPopup {
    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl Component for SendAsPopup {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> io::Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn update(&mut self, action: Action) {
        match action {
            Action::ShowSendAs(images, others) => {
                self.images = images;
                self.others = others;
                // Photos, like Telegram's apps
                self.selected = 0;
                self.visible = true;
            }
            Action::HideSendAs | Action::SendAsChosen(_) => {
                self.visible = false;
                self.last_popup = None;
            }
            Action::SendAsPrevious => self.move_selection(false),
            Action::SendAsNext => self.move_selection(true),
            Action::SendAsConfirm if self.visible => self.choose(self.selected),
            _ => {}
        }
    }

    /// The wheel moves the highlight; a click highlights a choice, and a
    /// click on the highlighted one sends.
    fn handle_mouse_events(&mut self, mouse: MouseEvent) -> io::Result<Option<Action>> {
        let Some((popup, offset)) = self.last_popup else {
            return Ok(None);
        };
        match popup_list::mouse_hit(mouse, popup, offset) {
            MouseHit::Previous => self.move_selection(false),
            MouseHit::Next => self.move_selection(true),
            MouseHit::Outside => return Ok(Some(Action::HideSendAs)),
            MouseHit::Line(index) if index == self.selected => self.choose(index),
            MouseHit::Line(index) if index < CHOICES.len() => self.selected = index,
            MouseHit::Line(_) | MouseHit::None => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect) -> io::Result<()> {
        if !self.visible {
            return Ok(());
        }
        let theme = self.app_context.theme();
        let rows: Vec<PopupRow> = CHOICES
            .iter()
            .map(|(send_as, label)| PopupRow {
                key: self.key_label(*send_as),
                label: vec![Span::raw(*label)],
                disabled: false,
            })
            .collect();
        self.last_popup = Some(popup_list::draw(
            frame,
            area,
            theme,
            &self.title(),
            &rows,
            self.selected,
            32,
        ));
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/send_as.rs"]
mod tests;
