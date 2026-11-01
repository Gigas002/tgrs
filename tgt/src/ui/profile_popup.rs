//! Popup showing a chat's profile: a user's name, usernames, phone and bio,
//! or a group's or channel's description and member count. Read only.

use crate::{
    app::action::Action,
    app::context::AppContext,
    settings::{BorderSurface, HighlightGroup, Theme},
    ui::component_traits::{Component, HandleFocus},
    ui::message,
};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use libtgt::model::{ChatKind, Profile};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::{io, sync::Arc};
use tokio::sync::mpsc::UnboundedSender;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Widest the popup gets, borders included.
const MAX_WIDTH: u16 = 64;
/// Widest the name gets in the popup title.
const TITLE_NAME_WIDTH: usize = 40;

/// The title of the popup: the chat's icon and name.
pub fn title(profile: &Profile) -> String {
    let icon = if profile.is_me {
        ChatKind::Private.icon()
    } else {
        profile.kind.icon()
    };
    let name = if profile.name.is_empty() {
        "Profile".to_owned()
    } else {
        message::truncate_display_width(&profile.name, TITLE_NAME_WIDTH)
    };
    if profile.is_me {
        format!("{icon} {name} (you)")
    } else {
        format!("{icon} {name}")
    }
}

/// Splits `text` into lines at most `width` wide, at spaces where it can and
/// at its own line breaks.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ') {
            let sep = usize::from(!line.is_empty());
            if line.width() + sep + word.width() <= width {
                if sep == 1 {
                    line.push(' ');
                }
                line.push_str(word);
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            // A word wider than a line is cut
            for c in word.chars() {
                if line.width() + c.width().unwrap_or(0) > width {
                    lines.push(std::mem::take(&mut line));
                }
                line.push(c);
            }
        }
        lines.push(line);
    }
    lines
}

/// The rows of `profile`, labels in a column before values wrapped to `width`.
pub fn profile_lines(theme: &Theme, profile: &Profile, width: usize) -> Vec<Line<'static>> {
    let fields = profile.fields();
    let label_width = fields
        .iter()
        .map(|(label, _)| label.width())
        .max()
        .unwrap_or(0);
    let value_width = width.saturating_sub(label_width + 2).max(8);
    let mut lines = Vec::new();
    for (label, value) in fields {
        for (index, part) in wrap(&value, value_width).into_iter().enumerate() {
            let label = if index == 0 { label } else { "" };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{label:>label_width$}  "),
                    theme.style(HighlightGroup::FieldLabel),
                ),
                Span::raw(part),
            ]));
        }
    }
    if !profile.is_loaded {
        lines.push(Line::styled(
            "Loading…",
            theme.style(HighlightGroup::Loading),
        ));
    }
    lines
}

pub struct ProfilePopup {
    app_context: Arc<AppContext>,
    name: String,
    action_tx: Option<UnboundedSender<Action>>,
    focused: bool,
    /// The chat whose profile is shown, while it's shown.
    chat_id: Option<i64>,
    profile: Option<Profile>,
    /// Lines scrolled past at the top.
    scroll: usize,
    /// Lines that don't fit, from the most recent draw.
    max_scroll: usize,
    /// Popup bounds from the most recent draw.
    last_popup: Option<Rect>,
}

impl ProfilePopup {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        ProfilePopup {
            app_context,
            name: String::new(),
            action_tx: None,
            focused: false,
            chat_id: None,
            profile: None,
            scroll: 0,
            max_scroll: 0,
            last_popup: None,
        }
    }

    pub fn with_name(mut self, name: impl AsRef<str>) -> Self {
        self.name = name.as_ref().to_string();
        self
    }

    /// Re-reads the profile from the store, which fills in as TDLib answers.
    fn refresh(&mut self) {
        self.profile = self
            .chat_id
            .and_then(|chat_id| self.app_context.store().profile(chat_id));
    }

    fn scroll_by(&mut self, down: bool) {
        self.scroll = if down {
            (self.scroll + 1).min(self.max_scroll)
        } else {
            self.scroll.saturating_sub(1)
        };
    }

    #[cfg(test)]
    fn shown(&self) -> Option<&Profile> {
        self.profile.as_ref()
    }
}

impl HandleFocus for ProfilePopup {
    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl Component for ProfilePopup {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> io::Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn update(&mut self, action: Action) {
        match action {
            Action::ShowProfileFor(chat_id) => {
                self.chat_id = Some(chat_id);
                self.scroll = 0;
                self.refresh();
            }
            Action::ProfileLoaded(chat_id) if self.chat_id == Some(chat_id) => self.refresh(),
            Action::HideProfile => {
                self.chat_id = None;
                self.profile = None;
            }
            Action::ProfileScrollDown => self.scroll_by(true),
            Action::ProfileScrollUp => self.scroll_by(false),
            _ => {}
        }
    }

    fn handle_mouse_events(&mut self, mouse: MouseEvent) -> io::Result<Option<Action>> {
        let Some(popup) = self.last_popup else {
            return Ok(None);
        };
        match mouse.kind {
            MouseEventKind::ScrollUp => self.scroll_by(false),
            MouseEventKind::ScrollDown => self.scroll_by(true),
            MouseEventKind::Down(MouseButton::Left) => {
                let inside = mouse.column >= popup.x
                    && mouse.column < popup.right()
                    && mouse.row >= popup.y
                    && mouse.row < popup.bottom();
                if !inside {
                    return Ok(Some(Action::HideProfile));
                }
            }
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect) -> io::Result<()> {
        self.refresh();
        let Some(profile) = self.profile.as_ref() else {
            return Ok(());
        };
        let theme = self.app_context.theme();
        let title = title(profile);
        let width = MAX_WIDTH.max(title.width() as u16 + 6).min(area.width);
        // Borders and a column of padding on both sides
        let lines = profile_lines(theme, profile, usize::from(width.saturating_sub(4)));
        let height = (lines.len() as u16 + 2).min(area.height);
        let popup = Rect::new(
            area.x + area.width.saturating_sub(width) / 2,
            area.y + area.height.saturating_sub(height) / 2,
            width,
            height,
        );
        let visible = usize::from(height.saturating_sub(2));
        self.max_scroll = lines.len().saturating_sub(visible);
        self.scroll = self.scroll.min(self.max_scroll);

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
        let content = Rect::new(
            inner.x + 1,
            inner.y,
            inner.width.saturating_sub(2),
            inner.height,
        );
        let shown: Vec<Line<'static>> = lines.into_iter().skip(self.scroll).collect();
        frame.render_widget(Paragraph::new(shown), content);
        self.last_popup = Some(popup);
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/profile_popup.rs"]
mod tests;
