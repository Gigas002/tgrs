use crate::{
    app::action::{Action, ChatCommand},
    app::context::AppContext,
    app::event::Event,
    settings::HighlightGroup,
    ui::component_name::ComponentName,
    ui::component_traits::{Component, HandleFocus},
    ui::message,
    ui::popup_list::{self, PopupRow},
};
use libtgt::model::{ChatKind, ChatState};
use ratatui::{Frame, layout::Rect, text::Span};
use std::{io, sync::Arc};
use tokio::sync::mpsc::UnboundedSender;

/// Widest the chat's name gets in the menu title.
const TITLE_NAME_WIDTH: usize = 30;

/// What running `command` on `chat_id` from a key does: commands that need a
/// confirmation open the menu waiting for it, others run right away.
pub fn chat_command_action(command: ChatCommand, chat_id: i64) -> Action {
    if command.needs_confirmation() {
        Action::ShowChatActionsFor(chat_id, Some(command))
    } else {
        Action::RunChatCommand(command, chat_id)
    }
}

/// What `command` does to a chat in `state`, e.g. `unmute`.
pub fn command_label(command: ChatCommand, state: &ChatState) -> &'static str {
    match command {
        ChatCommand::ToggleMute if state.is_muted => "unmute",
        ChatCommand::ToggleMute => "mute",
        ChatCommand::ToggleArchive if state.is_archived => "unarchive",
        ChatCommand::ToggleArchive => "archive",
        ChatCommand::TogglePin if state.is_pinned => "unpin chat",
        ChatCommand::TogglePin => "pin chat",
        ChatCommand::ToggleRead if state.is_unread => "mark as read",
        ChatCommand::ToggleRead => "mark as unread",
        ChatCommand::Remove => match state.kind {
            ChatKind::Channel => "leave channel",
            ChatKind::Group => "leave group",
            _ => "delete chat",
        },
    }
}

/// What happened to a chat that was in `state` once `command` ran, e.g. `unmuted`.
pub fn done_label(command: ChatCommand, state: &ChatState) -> &'static str {
    match command {
        ChatCommand::ToggleMute if state.is_muted => "unmuted",
        ChatCommand::ToggleMute => "muted",
        ChatCommand::ToggleArchive if state.is_archived => "unarchived",
        ChatCommand::ToggleArchive => "archived",
        ChatCommand::TogglePin if state.is_pinned => "unpinned",
        ChatCommand::TogglePin => "pinned",
        ChatCommand::ToggleRead if state.is_unread => "marked as read",
        ChatCommand::ToggleRead => "marked as unread",
        ChatCommand::Remove if state.is_left() => "left",
        ChatCommand::Remove => "deleted",
    }
}

/// One row of the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MenuItem {
    command: ChatCommand,
    label: &'static str,
    /// Why the action doesn't apply to the chat, if it doesn't.
    disabled: Option<&'static str>,
}

/// Popup listing what can be done with a chat, with the chat list key of each
/// action. Leaving or deleting the chat runs only when picked a second time.
pub struct ChatActions {
    app_context: Arc<AppContext>,
    name: String,
    action_tx: Option<UnboundedSender<Action>>,
    focused: bool,
    /// The chat the menu is for, while it's shown.
    chat: Option<ChatState>,
    items: Vec<MenuItem>,
    selected: usize,
    /// The command picked once, waiting to be picked again.
    armed: Option<ChatCommand>,
}

impl ChatActions {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        ChatActions {
            app_context,
            name: String::new(),
            action_tx: None,
            focused: false,
            chat: None,
            items: Vec::new(),
            selected: 0,
            armed: None,
        }
    }

    pub fn with_name(mut self, name: impl AsRef<str>) -> Self {
        self.name = name.as_ref().to_string();
        self
    }

    fn items_for(state: &ChatState) -> Vec<MenuItem> {
        ChatCommand::ALL
            .into_iter()
            .map(|command| MenuItem {
                command,
                label: command_label(command, state),
                disabled: match command {
                    ChatCommand::ToggleArchive if state.kind == ChatKind::Saved => {
                        Some("saved messages")
                    }
                    _ => None,
                },
            })
            .collect()
    }

    /// Shows the menu for `chat_id`, with `armed` selected and waiting for
    /// its confirmation.
    fn show(&mut self, chat_id: i64, armed: Option<ChatCommand>) {
        let Some(state) = self.app_context.store().chat_state(chat_id) else {
            self.chat = None;
            return;
        };
        self.items = Self::items_for(&state);
        self.chat = Some(state);
        self.armed = armed;
        self.selected = armed
            .and_then(|armed| self.items.iter().position(|item| item.command == armed))
            .unwrap_or(0);
    }

    fn move_selection(&mut self, forward: bool) {
        let len = self.items.len();
        if len == 0 {
            return;
        }
        self.armed = None;
        self.selected = if forward {
            (self.selected + 1) % len
        } else {
            (self.selected + len - 1) % len
        };
    }

    /// Runs the action at `index` on the chat and closes the menu; one that
    /// needs a confirmation waits for a second time.
    fn activate(&mut self, index: usize) {
        let (Some(item), Some(chat)) = (self.items.get(index).cloned(), self.chat.as_ref()) else {
            return;
        };
        let Some(tx) = self.action_tx.clone() else {
            return;
        };
        if let Some(reason) = item.disabled {
            let _ = tx.send(Action::StatusMessage(format!(
                "Can't {}: {reason}.",
                item.label
            )));
            return;
        }
        if item.command.needs_confirmation() && self.armed != Some(item.command) {
            self.armed = Some(item.command);
            return;
        }
        let chat_id = chat.chat_id;
        let _ = tx.send(Action::HideChatActions);
        let _ = tx.send(Action::RunChatCommand(item.command, chat_id));
    }

    /// The chat list keys bound to `command`, e.g. `m` to mute.
    fn chat_list_keys(&self, command: ChatCommand) -> Vec<Event> {
        let mut keys = self
            .app_context
            .keymap_config()
            .get_key_of_single_action(ComponentName::ChatList, Action::SelectedChat(command));
        keys.sort_by_key(|key| key.to_string());
        keys
    }

    fn title(&self) -> String {
        match self.chat.as_ref() {
            Some(chat) if !chat.title.is_empty() => {
                let name = message::truncate_display_width(&chat.title, TITLE_NAME_WIDTH);
                format!("{} {name}", chat.kind.icon())
            }
            _ => "Chat actions".to_owned(),
        }
    }

    #[cfg(test)]
    fn labels(&self) -> Vec<&'static str> {
        self.items.iter().map(|item| item.label).collect()
    }
}

impl HandleFocus for ChatActions {
    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl Component for ChatActions {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> io::Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn update(&mut self, action: Action) {
        match action {
            Action::ShowChatActionsFor(chat_id, armed) => self.show(chat_id, armed),
            Action::HideChatActions => {
                self.chat = None;
                self.armed = None;
            }
            Action::ChatActionsPrevious => self.move_selection(false),
            Action::ChatActionsNext => self.move_selection(true),
            Action::ChatActionsConfirm => self.activate(self.selected),
            // The chat list key of an action runs it directly
            Action::Key(key, modifiers) if self.chat.is_some() => {
                let event = Event::Key(key, modifiers.into());
                if let Some(index) = self
                    .items
                    .iter()
                    .position(|item| self.chat_list_keys(item.command).contains(&event))
                {
                    if index != self.selected {
                        self.armed = None;
                    }
                    self.selected = index;
                    self.activate(index);
                }
            }
            _ => {}
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect) -> io::Result<()> {
        if self.chat.is_none() || self.items.is_empty() {
            return Ok(());
        }
        let theme = self.app_context.theme();
        let rows: Vec<PopupRow> = self
            .items
            .iter()
            .map(|item| {
                let mut label = vec![Span::raw(item.label)];
                if let Some(reason) = item.disabled {
                    label.push(Span::styled(
                        format!(" ({reason})"),
                        theme.style(HighlightGroup::Muted),
                    ));
                }
                if self.armed == Some(item.command) {
                    label.push(Span::styled(
                        "  again to confirm",
                        theme.style(HighlightGroup::Warning),
                    ));
                }
                PopupRow {
                    key: self
                        .chat_list_keys(item.command)
                        .first()
                        .map(ToString::to_string)
                        .unwrap_or_default(),
                    label,
                    disabled: item.disabled.is_some(),
                }
            })
            .collect();
        popup_list::draw(frame, area, theme, &self.title(), &rows, self.selected, 28);
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/chat_actions.rs"]
mod tests;
