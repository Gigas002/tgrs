use crate::{
    app::action::{Action, MessageCommand},
    app::context::AppContext,
    app::event::Event,
    ui::component_name::ComponentName,
    ui::component_traits::{Component, HandleFocus},
};
use libtgt::model::MessageEntry;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::{io, sync::Arc};
use tokio::sync::mpsc::UnboundedSender;

/// One row of the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MenuItem {
    command: MessageCommand,
    /// Why the action doesn't apply to the message, if it doesn't.
    disabled: Option<&'static str>,
}

/// Popup listing what can be done with one chat message, with the chat key of
/// each action. Unavailable actions are dimmed.
pub struct MessageActions {
    app_context: Arc<AppContext>,
    name: String,
    action_tx: Option<UnboundedSender<Action>>,
    focused: bool,
    visible: bool,
    message_id: Option<i64>,
    items: Vec<MenuItem>,
    selected: usize,
}

impl MessageActions {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        MessageActions {
            app_context,
            name: String::new(),
            action_tx: None,
            focused: false,
            visible: false,
            message_id: None,
            items: Vec::new(),
            selected: 0,
        }
    }

    pub fn with_name(mut self, name: impl AsRef<str>) -> Self {
        self.name = name.as_ref().to_string();
        self
    }

    fn items_for(message: &MessageEntry, mine: bool) -> Vec<MenuItem> {
        MessageCommand::ALL
            .into_iter()
            .map(|command| {
                let disabled = match command {
                    MessageCommand::Reply => None,
                    MessageCommand::Edit if !mine => Some("not your message"),
                    MessageCommand::Edit if message.media().is_some() => Some("media message"),
                    MessageCommand::Edit => None,
                    MessageCommand::Copy if message.message_content_to_string().is_empty() => {
                        Some("no text")
                    }
                    MessageCommand::Copy => None,
                    MessageCommand::DeleteForEveryone | MessageCommand::DeleteForMe if !mine => {
                        Some("not your message")
                    }
                    MessageCommand::DeleteForEveryone | MessageCommand::DeleteForMe => None,
                    MessageCommand::OpenMedia
                    | MessageCommand::ViewMedia
                    | MessageCommand::DownloadMedia
                        if message.media().is_none() =>
                    {
                        Some("no media")
                    }
                    MessageCommand::OpenMedia
                    | MessageCommand::ViewMedia
                    | MessageCommand::DownloadMedia => None,
                };
                MenuItem { command, disabled }
            })
            .collect()
    }

    fn show(&mut self, message_id: i64) {
        let store = self.app_context.store();
        let Some(message) = store.get_message(message_id) else {
            self.visible = false;
            return;
        };
        let mine = message.sender_id() == store.me();
        self.items = Self::items_for(&message, mine);
        self.message_id = Some(message_id);
        // Start on the first action that applies
        self.selected = self
            .items
            .iter()
            .position(|item| item.disabled.is_none())
            .unwrap_or(0);
        self.visible = true;
    }

    fn hide(&mut self) {
        self.visible = false;
        // Keep the message selected in the chat
        if let Some(message_id) = self.message_id.take() {
            self.app_context
                .store()
                .set_jump_target_message_id_i64(message_id);
        }
    }

    fn move_selection(&mut self, forward: bool) {
        let len = self.items.len();
        if len == 0 {
            return;
        }
        self.selected = if forward {
            (self.selected + 1) % len
        } else {
            (self.selected + len - 1) % len
        };
    }

    /// Closes the menu and runs the action at `index` on the message.
    fn activate(&mut self, index: usize) {
        let (Some(item), Some(message_id)) = (self.items.get(index).cloned(), self.message_id)
        else {
            return;
        };
        let Some(tx) = self.action_tx.clone() else {
            return;
        };
        if let Some(reason) = item.disabled {
            let _ = tx.send(Action::StatusMessage(format!(
                "Can't {}: {reason}.",
                item.command.label()
            )));
            return;
        }
        let _ = tx.send(Action::HideMessageActions);
        let _ = tx.send(Action::RunMessageCommand(item.command, message_id));
    }

    /// The chat keys bound to `command`, e.g. `e` for edit.
    fn chat_keys(&self, command: MessageCommand) -> Vec<Event> {
        let mut keys = self
            .app_context
            .keymap_config()
            .get_key_of_single_action(ComponentName::Chat, Action::SelectedMessage(command));
        keys.sort_by_key(|key| key.to_string());
        keys
    }

    fn shortcut_label(&self, command: MessageCommand) -> String {
        self.chat_keys(command)
            .first()
            .map(ToString::to_string)
            .unwrap_or_default()
    }
}

impl HandleFocus for MessageActions {
    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl Component for MessageActions {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> io::Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn update(&mut self, action: Action) {
        match action {
            Action::ShowMessageActionsFor(message_id) => self.show(message_id),
            Action::HideMessageActions => self.hide(),
            Action::MessageActionsPrevious => self.move_selection(false),
            Action::MessageActionsNext => self.move_selection(true),
            Action::MessageActionsConfirm => self.activate(self.selected),
            // The chat key of an action runs it directly
            Action::Key(key, modifiers) if self.visible => {
                let event = Event::Key(key, modifiers.into());
                if let Some(index) = self
                    .items
                    .iter()
                    .position(|item| self.chat_keys(item.command).contains(&event))
                {
                    self.selected = index;
                    self.activate(index);
                }
            }
            _ => {}
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect) -> io::Result<()> {
        if !self.visible || self.items.is_empty() {
            return Ok(());
        }

        let rows: Vec<(String, &MenuItem)> = self
            .items
            .iter()
            .map(|item| (self.shortcut_label(item.command), item))
            .collect();
        let key_width = rows.iter().map(|(key, _)| key.len()).max().unwrap_or(0);
        let lines: Vec<Line<'_>> = rows
            .iter()
            .enumerate()
            .map(|(index, (key, item))| {
                let mut style = Style::default();
                if item.disabled.is_some() {
                    style = style.add_modifier(Modifier::DIM);
                }
                if index == self.selected {
                    style = style.patch(self.app_context.style_item_selected());
                }
                let label = match item.disabled {
                    Some(reason) => format!("{} ({reason})", item.command.label()),
                    None => item.command.label().to_owned(),
                };
                Line::from(vec![
                    Span::styled(
                        format!(" {key:>key_width$} "),
                        self.app_context.style_timestamp(),
                    ),
                    Span::styled(format!(" {label} "), style),
                ])
            })
            .collect();

        let content_width = lines.iter().map(Line::width).max().unwrap_or(0) as u16;
        let width = (content_width + 2).max(24).min(area.width);
        let height = (lines.len() as u16 + 2).min(area.height);
        let popup_area = Rect::new(
            area.x + area.width.saturating_sub(width) / 2,
            area.y + area.height.saturating_sub(height) / 2,
            width,
            height,
        );
        frame.render_widget(Clear, popup_area);

        let block = Block::new()
            .borders(Borders::ALL)
            .title("Message actions")
            .title_alignment(Alignment::Center)
            .border_style(self.app_context.style_border_component_focused())
            .style(self.app_context.style_chat());
        frame.render_widget(Paragraph::new(lines).block(block), popup_area);
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/message_actions.rs"]
mod tests;
