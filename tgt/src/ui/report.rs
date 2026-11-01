//! Popup reporting a chat or a message to Telegram's moderators, step by
//! step as Telegram asks: a reason (maybe narrowed down a few times), then
//! maybe a comment.

use crate::{
    app::action::{Action, ReportRequest},
    app::context::AppContext,
    settings::{BorderSurface, HighlightGroup},
    ui::component_traits::{Component, HandleFocus},
    ui::message,
    ui::popup_list::{self, PopupRow},
    ui::text_input::{self, TextInput},
};
use crossterm::event::{KeyCode, MouseButton, MouseEvent, MouseEventKind};
use libtgt::model::{ReportOption, ReportStep};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use std::{io, sync::Arc};
use tokio::sync::mpsc::UnboundedSender;

/// Widest the chat's name gets in the popup title.
const TITLE_NAME_WIDTH: usize = 30;
/// Widest a message popup gets, borders included.
const MESSAGE_WIDTH: u16 = 56;
/// Told when Telegram wants messages along with a chat report.
pub const NEEDS_MESSAGES: &str =
    "This chat can only be reported with its messages: select one in the chat and report it.";

/// Where the report stands.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Stage {
    /// Waiting for Telegram.
    Sending,
    Choose {
        title: String,
        options: Vec<ReportOption>,
        selected: usize,
    },
    Write {
        option_id: String,
        optional: bool,
        input: TextInput,
    },
    /// The report can't go on; why.
    Failed(String),
}

pub struct Report {
    app_context: Arc<AppContext>,
    name: String,
    action_tx: Option<UnboundedSender<Action>>,
    focused: bool,
    /// The chat and the messages reported, while the popup is shown.
    target: Option<(i64, Vec<i64>)>,
    stage: Stage,
    /// Popup bounds and first visible row from the most recent draw.
    last_popup: Option<(Rect, usize)>,
}

impl Report {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Report {
            app_context,
            name: String::new(),
            action_tx: None,
            focused: false,
            target: None,
            stage: Stage::Sending,
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

    /// Sends the next step of the report and waits for Telegram's answer.
    fn send_step(&mut self, option_id: String, text: String) {
        let Some((chat_id, message_ids)) = self.target.clone() else {
            return;
        };
        self.stage = Stage::Sending;
        self.send(Action::SendReport(ReportRequest {
            chat_id,
            message_ids,
            option_id,
            text,
        }));
    }

    fn show(&mut self, chat_id: i64, message_ids: Vec<i64>) {
        self.target = Some((chat_id, message_ids));
        self.send_step(String::new(), String::new());
    }

    fn stepped(&mut self, step: ReportStep) {
        self.stage = match step {
            ReportStep::ChooseOption { title, options } => Stage::Choose {
                title,
                options,
                selected: 0,
            },
            ReportStep::WriteText {
                option_id,
                optional,
            } => Stage::Write {
                option_id,
                optional,
                input: TextInput::default(),
            },
            ReportStep::NeedsMessages => Stage::Failed(NEEDS_MESSAGES.to_owned()),
            ReportStep::Done => {
                self.send(Action::HideReport);
                return;
            }
        };
    }

    fn move_selection(&mut self, forward: bool) {
        if let Stage::Choose {
            options, selected, ..
        } = &mut self.stage
        {
            let len = options.len();
            if len > 0 {
                *selected = if forward {
                    (*selected + 1) % len
                } else {
                    (*selected + len - 1) % len
                };
            }
        }
    }

    /// Picks the highlighted reason, or sends the comment.
    fn confirm(&mut self) {
        match &self.stage {
            Stage::Choose {
                options, selected, ..
            } => {
                if let Some(option) = options.get(*selected) {
                    let option_id = option.id.clone();
                    self.send_step(option_id, String::new());
                }
            }
            Stage::Write {
                option_id,
                optional,
                input,
            } => {
                let text = input.text().trim().to_owned();
                if text.is_empty() && !optional {
                    return;
                }
                let option_id = option_id.clone();
                self.send_step(option_id, text);
            }
            Stage::Sending | Stage::Failed(_) => {}
        }
    }

    /// What's reported, for the popup title.
    fn subject(&self) -> String {
        let Some((chat_id, message_ids)) = self.target.as_ref() else {
            return "Report".to_owned();
        };
        let name = self
            .app_context
            .store()
            .name_from_chats(*chat_id)
            .map(|name| message::truncate_display_width(&name, TITLE_NAME_WIDTH))
            .unwrap_or_default();
        match (message_ids.len(), name.is_empty()) {
            (0, false) => format!("Report {name}"),
            (0, true) => "Report chat".to_owned(),
            (1, _) => "Report message".to_owned(),
            (n, _) => format!("Report {n} messages"),
        }
    }

    /// Draws a popup with just `text` in it.
    fn draw_message(&self, frame: &mut Frame<'_>, area: Rect, text: Span<'static>) -> Rect {
        let theme = self.app_context.theme();
        let width = MESSAGE_WIDTH.min(area.width);
        let inner_width = usize::from(width.saturating_sub(4)).max(1);
        let rows = text.content.chars().count().div_ceil(inner_width).max(1) as u16;
        let height = (rows + 2).min(area.height);
        let popup = Rect::new(
            area.x + area.width.saturating_sub(width) / 2,
            area.y + area.height.saturating_sub(height) / 2,
            width,
            height,
        );
        frame.render_widget(Clear, popup);
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(theme.border_type(BorderSurface::Modal))
            .border_style(theme.style(HighlightGroup::ModalBorder))
            .title(Span::styled(
                format!(" {} ", self.subject()),
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
        frame.render_widget(
            Paragraph::new(Line::from(text)).wrap(Wrap { trim: true }),
            content,
        );
        popup
    }
}

impl HandleFocus for Report {
    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl Component for Report {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> io::Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn update(&mut self, action: Action) {
        match action {
            Action::ShowReport(chat_id, message_ids) => self.show(chat_id, message_ids),
            Action::HideReport => {
                self.target = None;
                self.stage = Stage::Sending;
            }
            Action::ReportStepped(step) if self.target.is_some() => self.stepped(step),
            Action::ReportFailed(message) if self.target.is_some() => {
                self.stage = Stage::Failed(message);
            }
            Action::ReportNext => self.move_selection(true),
            Action::ReportPrevious => self.move_selection(false),
            Action::ReportConfirm => self.confirm(),
            Action::Key(key, modifiers) => match &mut self.stage {
                Stage::Write { input, .. } => {
                    input.handle_key(key, modifiers);
                }
                Stage::Choose { options, .. } => match key {
                    KeyCode::Char('j') => self.move_selection(true),
                    KeyCode::Char('k') => self.move_selection(false),
                    // 1-9 pick a reason directly
                    KeyCode::Char(c @ '1'..='9') => {
                        let index = usize::from(c as u8 - b'1');
                        if let Some(option) = options.get(index) {
                            let option_id = option.id.clone();
                            self.send_step(option_id, String::new());
                        }
                    }
                    _ => {}
                },
                _ => {}
            },
            Action::Paste(text) => {
                if let Stage::Write { input, .. } = &mut self.stage {
                    input.insert(&text);
                }
            }
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
                    return Ok(Some(Action::HideReport));
                }
                if let Stage::Choose {
                    options, selected, ..
                } = &mut self.stage
                    && mouse.row > popup.y
                    && mouse.row + 1 < popup.bottom()
                {
                    let index = offset + usize::from(mouse.row - popup.y - 1);
                    if index < options.len() {
                        if *selected == index {
                            self.confirm();
                        } else {
                            *selected = index;
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect) -> io::Result<()> {
        if self.target.is_none() {
            return Ok(());
        }
        let theme = self.app_context.theme();
        let popup = match &self.stage {
            Stage::Sending => (
                self.draw_message(
                    frame,
                    area,
                    Span::styled("Sending…", theme.style(HighlightGroup::Loading)),
                ),
                0,
            ),
            Stage::Failed(message) => (
                self.draw_message(
                    frame,
                    area,
                    Span::styled(message.clone(), theme.style(HighlightGroup::Warning)),
                ),
                0,
            ),
            Stage::Choose {
                title,
                options,
                selected,
            } => {
                let rows: Vec<PopupRow> = options
                    .iter()
                    .enumerate()
                    .map(|(index, option)| PopupRow {
                        key: if index < 9 {
                            (index + 1).to_string()
                        } else {
                            String::new()
                        },
                        label: vec![Span::raw(option.text.clone())],
                        disabled: false,
                    })
                    .collect();
                let title = if title.is_empty() {
                    self.subject()
                } else {
                    title.clone()
                };
                popup_list::draw(frame, area, theme, &title, &rows, *selected, 36)
            }
            Stage::Write {
                optional, input, ..
            } => {
                let hint = if *optional {
                    "optional; enter sends the report"
                } else {
                    "enter sends the report"
                };
                (
                    text_input::draw_modal(
                        frame,
                        area,
                        theme,
                        &self.subject(),
                        "Details: ",
                        input,
                        Span::styled(hint, theme.style(HighlightGroup::Hint)),
                        self.focused,
                    ),
                    0,
                )
            }
        };
        self.last_popup = Some(popup);
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/report.rs"]
mod tests;
