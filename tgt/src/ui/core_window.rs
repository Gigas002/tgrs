use crate::{
    app::action::Action,
    app::context::AppContext,
    app::error::AppError,
    app::event::Event,
    settings::ActionBinding,
    ui::component_name::ComponentName,
    ui::{
        MAX_CHAT_LIST_SIZE, MAX_PROMPT_SIZE, MAX_SIDE_PANES_SIZE, MIN_CHAT_LIST_SIZE,
        MIN_PROMPT_SIZE,
    },
    ui::{
        archive_list_window::ArchiveListWindow,
        blocked_list::BlockedList,
        chat_actions::{ChatActions, chat_command_action},
        chat_list_window::{ChatListWindow, open_chat},
        chat_window::ChatWindow,
        command_guide::CommandGuide,
        component_traits::{Component, HandleFocus},
        edit_profile::EditProfile,
        file_upload_explorer::FileUploadExplorer,
        find_chat::FindChat,
        forward::Forward,
        leader_menu::LeaderMenu,
        message_actions::MessageActions,
        picker::Picker,
        pinned_messages_popup::PinnedMessagesPopup,
        profile_popup::ProfilePopup,
        prompt_window::PromptWindow,
        report::Report,
        search_overlay::SearchOverlay,
        sessions::Sessions,
        sticker_picker::StickerPicker,
        topic_list_window::TopicListWindow,
    },
};
use crossterm::event::{MouseButton, MouseEventKind};
use libtgt::model::ChatListEntry;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use std::{collections::HashMap, io, sync::Arc};
use tokio::sync::mpsc::UnboundedSender;

use super::reply_message::ReplyMessage;

/// Width of the chat list, the archived chats and the topic list when shown, in percent.
const DEFAULT_SIDE_PANE_SIZE: u16 = 20;

/// `CoreWindow` is a struct that represents the core window of the application.
/// It is responsible for managing the layout and rendering of the core window.
pub struct CoreWindow {
    /// The application context.
    app_context: Arc<AppContext>,
    /// The name of the `CoreWindow`.
    name: String,
    /// An unbounded sender that send action for processing.
    action_tx: Option<UnboundedSender<Action>>,
    /// A map of components that are part of the `CoreWindow`.
    components: HashMap<ComponentName, Box<dyn Component>>,
    /// A flag indicating whether the `CoreWindow` should be displayed as a
    /// smaller version of itself.
    small_area: bool,
    /// The size of the prompt component.
    size_prompt: u16,
    /// Rows added to the prompt by hand (Alt+Up/Down), on top of the rows
    /// its text needs.
    prompt_extra_rows: u16,
    /// The pane focused when the leader menu opened, focused again after.
    focus_before_leader: Option<ComponentName>,
    /// The pane focused when the sticker picker opened, focused again after.
    focus_before_stickers: Option<ComponentName>,
    /// The pane focused when the chat actions opened, focused again after.
    focus_before_chat_actions: Option<ComponentName>,
    /// The pane focused when the profile opened, focused again after.
    focus_before_profile: Option<ComponentName>,
    /// The pane focused when the username popup opened, focused again after.
    focus_before_find_chat: Option<ComponentName>,
    /// The pane focused when the profile editor opened, focused again after.
    focus_before_edit_profile: Option<ComponentName>,
    /// Whether the profile editor was opened from your profile, shown
    /// again once the edit is saved.
    edit_profile_from_profile: bool,
    /// The chat whose profile is shown, while it's shown.
    profile_chat: Option<i64>,
    /// The pane focused when the report popup opened, focused again after.
    focus_before_report: Option<ComponentName>,
    /// The pane focused when the blocked list opened, focused again after.
    focus_before_blocked_list: Option<ComponentName>,
    /// The pane focused when the session list opened, focused again after.
    focus_before_sessions: Option<ComponentName>,
    /// The pane focused when the forward popup opened, focused again after.
    focus_before_forward: Option<ComponentName>,
    /// The size of the message reply component.
    size_message_reply: u16,
    /// The size of the chat list component.
    size_chat_list: u16,
    /// The size of the topic list, shown while the open chat is a forum; 0 hides it.
    size_topic_list: u16,
    /// The size of the archived chats when shown.
    size_archive_list: u16,
    /// Whether the archived chats are shown, after opening the archive row.
    archive_list_shown: bool,
    /// The name of the component that currently has focus. It is an optional
    /// value because no component may have focus. The focus is a component
    /// inside the `CoreWindow`.
    component_focused: Option<ComponentName>,
    /// Indicates whether the `CoreWindow` is focused or not.
    focused: bool,
    /// Indicates whether the reply message should be shown.
    show_reply_message: bool,
    /// The popup drawn over the window, if any; only one is shown at a time.
    active_popup: Option<ComponentName>,
    /// Last known screen areas for focusable sections (chat list, chat, prompt) for click-to-focus.
    last_focusable_areas: HashMap<ComponentName, Rect>,
}

impl CoreWindow {
    /// Create a new instance of the `CoreWindow` struct.
    ///
    /// # Arguments
    /// * `app_context` - An Arc wrapped AppContext struct.
    ///
    /// # Returns
    /// * `Self` - The new instance of the `CoreWindow` struct.
    pub fn new(app_context: Arc<AppContext>) -> Self {
        let components_iter: Vec<(ComponentName, Box<dyn Component>)> = vec![
            (
                ComponentName::ChatList,
                ChatListWindow::new(Arc::clone(&app_context))
                    .with_name(ComponentName::ChatList.to_string())
                    .new_boxed(),
            ),
            (
                ComponentName::ArchiveList,
                ArchiveListWindow::new(Arc::clone(&app_context))
                    .with_name(ComponentName::ArchiveList.to_string())
                    .new_boxed(),
            ),
            (
                ComponentName::TopicList,
                TopicListWindow::new(Arc::clone(&app_context))
                    .with_name(ComponentName::TopicList.to_string())
                    .new_boxed(),
            ),
            (
                ComponentName::Chat,
                ChatWindow::new(Arc::clone(&app_context))
                    .with_name(ComponentName::Chat.to_string())
                    .new_boxed(),
            ),
            (
                ComponentName::Prompt,
                PromptWindow::new(Arc::clone(&app_context))
                    .with_name(ComponentName::Prompt.to_string())
                    .new_boxed(),
            ),
            (
                ComponentName::ReplyMessage,
                ReplyMessage::new(Arc::clone(&app_context))
                    .with_name(ComponentName::ReplyMessage.to_string())
                    .new_boxed(),
            ),
            (
                ComponentName::CommandGuide,
                CommandGuide::new(Arc::clone(&app_context))
                    .with_name(ComponentName::CommandGuide.to_string())
                    .new_boxed(),
            ),
            (
                ComponentName::SearchOverlay,
                SearchOverlay::new(Arc::clone(&app_context))
                    .with_name(ComponentName::SearchOverlay.to_string())
                    .new_boxed(),
            ),
            (
                ComponentName::MessageActions,
                MessageActions::new(Arc::clone(&app_context))
                    .with_name(ComponentName::MessageActions.to_string())
                    .new_boxed(),
            ),
            (
                ComponentName::LeaderMenu,
                LeaderMenu::new(Arc::clone(&app_context))
                    .with_name(ComponentName::LeaderMenu.to_string())
                    .new_boxed(),
            ),
            (
                ComponentName::Picker,
                Picker::new(Arc::clone(&app_context))
                    .with_name(ComponentName::Picker.to_string())
                    .new_boxed(),
            ),
            (
                ComponentName::ChatActions,
                ChatActions::new(Arc::clone(&app_context))
                    .with_name(ComponentName::ChatActions.to_string())
                    .new_boxed(),
            ),
            (
                ComponentName::Report,
                Report::new(Arc::clone(&app_context))
                    .with_name(ComponentName::Report.to_string())
                    .new_boxed(),
            ),
            (
                ComponentName::Forward,
                Forward::new(Arc::clone(&app_context))
                    .with_name(ComponentName::Forward.to_string())
                    .new_boxed(),
            ),
            (
                ComponentName::Sessions,
                Sessions::new(Arc::clone(&app_context))
                    .with_name(ComponentName::Sessions.to_string())
                    .new_boxed(),
            ),
            (
                ComponentName::BlockedList,
                BlockedList::new(Arc::clone(&app_context))
                    .with_name(ComponentName::BlockedList.to_string())
                    .new_boxed(),
            ),
            (
                ComponentName::FindChat,
                FindChat::new(Arc::clone(&app_context))
                    .with_name(ComponentName::FindChat.to_string())
                    .new_boxed(),
            ),
            (
                ComponentName::EditProfile,
                EditProfile::new(Arc::clone(&app_context))
                    .with_name(ComponentName::EditProfile.to_string())
                    .new_boxed(),
            ),
            (
                ComponentName::Profile,
                ProfilePopup::new(Arc::clone(&app_context))
                    .with_name(ComponentName::Profile.to_string())
                    .new_boxed(),
            ),
            (
                ComponentName::StickerPicker,
                StickerPicker::new(Arc::clone(&app_context))
                    .with_name(ComponentName::StickerPicker.to_string())
                    .new_boxed(),
            ),
            (
                ComponentName::FileUploadExplorer,
                FileUploadExplorer::new(Arc::clone(&app_context))
                    .with_name(ComponentName::FileUploadExplorer.to_string())
                    .new_boxed(),
            ),
            (
                ComponentName::PinnedMessagesPopup,
                PinnedMessagesPopup::new(Arc::clone(&app_context))
                    .with_name(ComponentName::PinnedMessagesPopup.to_string())
                    .new_boxed(),
            ),
        ];

        let app_context = app_context;
        let name = "".to_string();
        let action_tx = None;
        let components: HashMap<ComponentName, Box<dyn Component>> =
            components_iter.into_iter().collect();
        let size_prompt = 3;
        let size_message_reply = 3;
        let size_chat_list = DEFAULT_SIDE_PANE_SIZE;
        let small_area = false;
        // Some pane always has the focus; the chat list to start with
        let component_focused = Some(ComponentName::ChatList);
        let focused = true;
        let show_reply_message = false;
        let last_focusable_areas = HashMap::new();

        let mut window = CoreWindow {
            app_context,
            name,
            action_tx,
            components,
            size_chat_list,
            size_topic_list: DEFAULT_SIDE_PANE_SIZE,
            size_archive_list: DEFAULT_SIDE_PANE_SIZE,
            archive_list_shown: false,
            size_prompt,
            prompt_extra_rows: 0,
            focus_before_leader: None,
            focus_before_stickers: None,
            focus_before_chat_actions: None,
            focus_before_profile: None,
            focus_before_find_chat: None,
            focus_before_edit_profile: None,
            edit_profile_from_profile: false,
            profile_chat: None,
            focus_before_report: None,
            focus_before_blocked_list: None,
            focus_before_sessions: None,
            focus_before_forward: None,
            size_message_reply,
            small_area,
            component_focused,
            focused,
            show_reply_message,
            active_popup: None,
            last_focusable_areas,
        };
        window.focus_only(component_focused);
        window
    }
    /// Set the name of the `CoreWindow`.
    ///
    /// # Arguments
    /// * `name` - The name of the `CoreWindow`.
    ///
    /// # Returns
    /// * `Self` - The modified instance of the `CoreWindow`.
    pub fn with_name(mut self, name: impl AsRef<str>) -> Self {
        self.name = name.as_ref().to_string();
        self
    }

    /// Set small area flag.
    ///
    /// # Arguments
    /// * `small_area` - A flag indicating whether the `CoreWindow` should be displayed as a
    pub fn with_small_area(&mut self, small_area: bool) {
        self.small_area = small_area;
    }

    /// Toggle the chat list component.
    pub fn toggle_chat_list(&mut self) {
        self.size_chat_list = if self.size_chat_list == 0 {
            DEFAULT_SIDE_PANE_SIZE.min(MAX_SIDE_PANES_SIZE.saturating_sub(self.side_panes_size()))
        } else {
            0
        };
    }
    /// Increase the size of the chat list component.
    pub fn increase_chat_list_size(&mut self) {
        self.resize_side_pane(ComponentName::ChatList, true);
    }

    /// Whether the topic list is drawn: the open chat is a forum and it isn't hidden.
    fn topic_list_shown(&self) -> bool {
        self.size_topic_list > 0 && self.app_context.store().open_chat_is_forum()
    }

    /// Hides or shows the topic list.
    pub fn toggle_topic_list(&mut self) {
        self.size_topic_list = if self.size_topic_list == 0 {
            DEFAULT_SIDE_PANE_SIZE.min(MAX_SIDE_PANES_SIZE.saturating_sub(self.side_panes_size()))
        } else {
            0
        };
    }

    /// Shows the archived chats, which start at the open chat again.
    fn show_archive_list(&mut self) {
        if self.archive_list_shown {
            return;
        }
        self.archive_list_shown = true;
        if let Some(archive) = self.components.get_mut(&ComponentName::ArchiveList) {
            archive.update(Action::FocusComponent(ComponentName::ArchiveList));
        }
    }

    /// Hides the archived chats; their focus goes to the chat list.
    fn hide_archive_list(&mut self) {
        self.archive_list_shown = false;
        if self.component_focused == Some(ComponentName::ArchiveList) {
            let next = if self.size_chat_list > 0 {
                ComponentName::ChatList
            } else {
                ComponentName::Chat
            };
            self.focus_only(Some(next));
        }
    }

    /// The side panes shown left of the chat, from the left.
    fn side_panes(&self) -> Vec<ComponentName> {
        let mut panes = Vec::new();
        if self.size_chat_list > 0 {
            panes.push(ComponentName::ChatList);
        }
        if self.archive_list_shown {
            panes.push(ComponentName::ArchiveList);
        }
        if self.topic_list_shown() {
            panes.push(ComponentName::TopicList);
        }
        panes
    }

    /// The width a side pane takes, 0 while it isn't shown.
    fn side_pane_width(&self, pane: ComponentName) -> u16 {
        match pane {
            ComponentName::ChatList => self.size_chat_list,
            ComponentName::ArchiveList if self.archive_list_shown => self.size_archive_list,
            ComponentName::TopicList if self.topic_list_shown() => self.size_topic_list,
            _ => 0,
        }
    }

    /// The width the shown side panes take together.
    fn side_panes_size(&self) -> u16 {
        self.side_panes()
            .into_iter()
            .map(|pane| self.side_pane_width(pane))
            .sum()
    }

    fn side_pane_size_mut(&mut self, pane: ComponentName) -> Option<&mut u16> {
        match pane {
            ComponentName::ChatList => Some(&mut self.size_chat_list),
            ComponentName::ArchiveList => Some(&mut self.size_archive_list),
            ComponentName::TopicList => Some(&mut self.size_topic_list),
            _ => None,
        }
    }

    /// Makes a side pane wider (`true`) or narrower, within the chat list's
    /// limits and leaving the chat its share of the width.
    fn resize_side_pane(&mut self, pane: ComponentName, wider: bool) {
        let full = self.side_panes_size() >= MAX_SIDE_PANES_SIZE;
        let Some(size) = self.side_pane_size_mut(pane) else {
            return;
        };
        if wider {
            if *size < MAX_CHAT_LIST_SIZE && !full {
                *size += 1;
            }
        } else if *size > MIN_CHAT_LIST_SIZE {
            *size -= 1;
        }
    }

    /// Narrows the side panes, the leftmost first, until they leave the chat
    /// its share of the width: a forum may open while they are wide.
    fn fit_side_panes(&mut self) {
        for pane in self.side_panes() {
            let excess = self.side_panes_size().saturating_sub(MAX_SIDE_PANES_SIZE);
            if excess == 0 {
                break;
            }
            if let Some(size) = self.side_pane_size_mut(pane) {
                *size -= excess.min(size.saturating_sub(MIN_CHAT_LIST_SIZE));
            }
        }
    }

    /// Increase the size of the chat list component.
    pub fn increase_size_prompt(&mut self) {
        if self.size_prompt == MAX_PROMPT_SIZE {
            return;
        }
        self.size_prompt += 1;
    }
    /// Decrease the size of the chat list component.
    pub fn decrease_chat_list_size(&mut self) {
        self.resize_side_pane(ComponentName::ChatList, false);
    }
    /// Decrease the size of the chat list component.
    pub fn decrease_size_prompt(&mut self) {
        if self.size_prompt == MIN_PROMPT_SIZE {
            return;
        }
        self.size_prompt -= 1;
    }

    /// Panes in the order Left/Right move through; hidden lists are skipped.
    fn panes(&self) -> Vec<ComponentName> {
        let mut panes = self.side_panes();
        panes.extend([ComponentName::Chat, ComponentName::Prompt]);
        panes
    }

    /// Focuses the pane `step` panes away from the focused one, stopping at
    /// the ends. With nothing focused, the chat.
    fn focus_pane(&mut self, step: isize) {
        let panes = self.panes();
        let next = match self
            .component_focused
            .and_then(|focused| panes.iter().position(|pane| *pane == focused))
        {
            Some(index) => index
                .checked_add_signed(step)
                .and_then(|index| panes.get(index))
                .copied(),
            None => Some(ComponentName::Chat),
        };
        if let Some(next) = next {
            self.focus_only(Some(next));
        }
    }

    /// Like concord: Alt+Right makes the focused pane wider, Alt+Left narrower.
    /// The chat and the prompt share the right side, so they grow together,
    /// taking from the pane left of them.
    fn resize_focused_width(&mut self, wider: bool) {
        let side_panes = self.side_panes();
        match self.component_focused {
            Some(ComponentName::Chat | ComponentName::Prompt) => {
                if let Some(left) = side_panes.last() {
                    self.resize_side_pane(*left, !wider);
                }
            }
            Some(pane) if side_panes.contains(&pane) => self.resize_side_pane(pane, wider),
            _ => {}
        }
    }

    /// Alt+Up makes the focused pane taller, Alt+Down shorter: the chat and
    /// the prompt trade rows.
    fn resize_focused_height(&mut self, taller: bool) {
        let prompt_grows = match self.component_focused {
            Some(ComponentName::Prompt) => taller,
            Some(ComponentName::Chat) => !taller,
            _ => return,
        };
        if prompt_grows {
            if self.size_prompt + self.prompt_extra_rows < MAX_PROMPT_SIZE {
                self.prompt_extra_rows += 1;
            }
        } else {
            self.prompt_extra_rows = self.prompt_extra_rows.saturating_sub(1);
        }
    }

    /// The action that tells a popup it is being hidden.
    fn hide_action(popup: ComponentName) -> Option<Action> {
        Some(match popup {
            ComponentName::CommandGuide => Action::HideCommandGuide,
            ComponentName::SearchOverlay => Action::CloseSearchOverlay,
            ComponentName::MessageActions => Action::HideMessageActions,
            ComponentName::FileUploadExplorer => Action::HideFileUploadExplorer,
            ComponentName::PinnedMessagesPopup => Action::HidePinnedMessagesPopup,
            ComponentName::Picker => Action::HidePicker,
            ComponentName::StickerPicker => Action::HideStickerPicker,
            ComponentName::ChatActions => Action::HideChatActions,
            ComponentName::Profile => Action::HideProfile,
            ComponentName::FindChat => Action::HideFindChat,
            ComponentName::EditProfile => Action::HideEditProfile,
            ComponentName::Report => Action::HideReport,
            ComponentName::BlockedList => Action::HideBlockedList,
            ComponentName::Sessions => Action::HideSessions,
            ComponentName::Forward => Action::HideForward,
            ComponentName::LeaderMenu => Action::HideLeaderMenu,
            _ => return None,
        })
    }

    fn is_popup_visible(&self, popup: ComponentName) -> bool {
        self.active_popup == Some(popup)
    }

    /// The pane focused when no other one is: the chat list, or the chat
    /// while the chat list is hidden.
    fn default_pane(&self) -> ComponentName {
        if self.size_chat_list > 0 {
            ComponentName::ChatList
        } else {
            ComponentName::Chat
        }
    }

    /// Focuses `name` and unfocuses every other component. Some pane always
    /// has the focus: `None` focuses the [default pane](Self::default_pane).
    fn focus_only(&mut self, name: Option<ComponentName>) {
        let name = Some(name.unwrap_or_else(|| self.default_pane()));
        self.component_focused = name;
        self.app_context.set_focused_component(name);
        for (component_name, component) in self.components.iter_mut() {
            if Some(*component_name) == name {
                component.focus();
            } else {
                component.unfocus();
            }
        }
    }

    /// Hides `popup` if it is visible, without moving the focus.
    fn hide_popup(&mut self, popup: ComponentName) {
        if !self.is_popup_visible(popup) {
            return;
        }
        self.active_popup = None;
        if let Some(component) = self.components.get_mut(&popup) {
            if let Some(action) = Self::hide_action(popup) {
                component.update(action);
            }
            component.unfocus();
        }
    }

    /// Shows `popup` on top of the window and focuses it, so its keymap is active.
    fn show_popup(&mut self, popup: ComponentName, action: Action) {
        if let Some(active) = self.active_popup
            && active != popup
        {
            self.hide_popup(active);
        }
        self.active_popup = Some(popup);
        self.focus_only(Some(popup));
        if let Some(component) = self.components.get_mut(&popup) {
            component.update(action);
        }
    }

    /// Hides `popup` and, when it had the focus, moves the focus to `refocus`.
    fn close_popup(&mut self, popup: ComponentName, refocus: Option<ComponentName>) {
        let had_focus = self.component_focused == Some(popup);
        if self.is_popup_visible(popup) {
            self.hide_popup(popup);
        } else if let Some(action) = Self::hide_action(popup)
            && let Some(component) = self.components.get_mut(&popup)
        {
            component.update(action);
        }
        if had_focus {
            self.focus_only(refocus);
        }
    }

    /// Shows `popup`, or closes it when it is already visible.
    fn toggle_popup(
        &mut self,
        popup: ComponentName,
        action: Action,
        refocus: Option<ComponentName>,
    ) {
        if self.is_popup_visible(popup) {
            self.close_popup(popup, refocus);
        } else {
            self.show_popup(popup, action);
        }
    }
}
/// Implement the `HandleFocus` trait for the `CoreWindow` struct.
/// This trait allows the `CoreWindow` to be focused or unfocused.
impl HandleFocus for CoreWindow {
    /// Set the `focused` flag for the `CoreWindow`.
    fn focus(&mut self) {
        self.focused = true;
    }
    /// Set the `focused` flag for the `CoreWindow`.
    fn unfocus(&mut self) {
        self.focused = false;
    }
}

/// Implement the `Component` trait for the `ChatListWindow` struct.
impl Component for CoreWindow {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> std::io::Result<()> {
        self.action_tx = Some(tx.clone());
        for component in self.components.values_mut() {
            component.register_action_handler(tx.clone())?;
        }
        Ok(())
    }

    fn handle_events(&mut self, event: Option<Event>) -> Result<Option<Action>, AppError> {
        let Some(ev) = event else {
            return Ok(None);
        };
        // Forward mouse events to the active popup before checking the panes below it.
        if let Event::Mouse(mouse) = ev {
            // Popups own mouse clicks while visible. Let them handle clicks inside
            // their bounds and dismiss themselves for clicks outside; otherwise
            // the normal pane focus logic would close them on every click.
            if let Some(popup) = self.active_popup
                && let Some(component) = self.components.get_mut(&popup)
            {
                return component.handle_mouse_events(mouse).map_err(AppError::from);
            }
            // Left click: focus the section under the cursor (chat list, chat window, prompt)
            if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
                let col = mouse.column;
                let row = mouse.row;
                for name in [
                    ComponentName::ChatList,
                    ComponentName::ArchiveList,
                    ComponentName::TopicList,
                    ComponentName::Chat,
                    ComponentName::Prompt,
                ] {
                    if let Some(rect) = self.last_focusable_areas.get(&name) {
                        let in_rect = col >= rect.x
                            && col < rect.x + rect.width
                            && row >= rect.y
                            && row < rect.y + rect.height;
                        if in_rect && self.component_focused != Some(name) {
                            return Ok(Some(Action::FocusComponent(name)));
                        }
                    }
                }
            }
            if let Some(name) = self.component_focused
                && let Some(component) = self.components.get_mut(&name)
            {
                return component.handle_mouse_events(mouse).map_err(AppError::from);
            }
            return Ok(None);
        }
        let binding = self.app_context.keymap_config();
        let map = binding.get_map_of(self.component_focused);
        if let Some(action_binding) = map.get(&ev) {
            match action_binding {
                ActionBinding::Single { action, .. } => {
                    return Ok(Some(action.clone()));
                }
                ActionBinding::Multiple(_map_event_action) => {
                    tracing::warn!(
                        "Multiple action bindings are not supported yet. They are supported only for default key bindings because there are not the app context to handle them here."
                    );
                    todo!();
                }
            }
        }
        Ok(Some(Action::Unknown))
    }

    fn update(&mut self, action: Action) {
        match action {
            // The topic list is only there for a forum; focusing it shows it again
            Action::FocusComponent(ComponentName::TopicList)
                if !self.app_context.store().open_chat_is_forum() => {}
            Action::FocusComponent(component_name) => {
                if component_name == ComponentName::TopicList && self.size_topic_list == 0 {
                    self.toggle_topic_list();
                }
                if component_name == ComponentName::ArchiveList {
                    self.show_archive_list();
                }
                // If the previously focused component was a popup, hide it automatically
                if let Some(prev_focused) = self.component_focused
                    && prev_focused.is_popup()
                    && prev_focused != component_name
                {
                    self.hide_popup(prev_focused);
                }

                self.focus_only(Some(component_name));
            }
            // Esc closes a popup or leaves the prompt; a pane keeps the focus
            Action::UnfocusComponent => {
                self.show_reply_message = false;
                if let Some(popup) = self.active_popup {
                    // Its own hide action gives the focus back where it was
                    match Self::hide_action(popup) {
                        Some(hide) => self.update(hide),
                        None => self.hide_popup(popup),
                    }
                }
                match self.component_focused {
                    // Leaving the prompt goes back to the messages, like concord
                    Some(ComponentName::Prompt) => self.focus_only(Some(ComponentName::Chat)),
                    Some(focused) if !focused.is_popup() => {}
                    _ => self.focus_only(None),
                }
            }
            Action::FocusNextPane => self.focus_pane(1),
            Action::FocusPreviousPane => self.focus_pane(-1),
            Action::ResizePaneWider => self.resize_focused_width(true),
            Action::ResizePaneNarrower => self.resize_focused_width(false),
            Action::ResizePaneTaller => self.resize_focused_height(true),
            Action::ResizePaneShorter => self.resize_focused_height(false),
            Action::ToggleChatList => {
                self.toggle_chat_list();
                // A hidden chat list can't keep the focus
                if self.size_chat_list == 0
                    && self.component_focused == Some(ComponentName::ChatList)
                {
                    self.focus_only(Some(ComponentName::Chat));
                }
            }
            Action::ToggleArchiveList => {
                if self.archive_list_shown {
                    self.hide_archive_list();
                } else {
                    self.show_archive_list();
                }
            }
            Action::CloseArchiveList => self.hide_archive_list(),
            Action::ToggleTopicList => {
                self.toggle_topic_list();
                // A hidden topic list can't keep the focus
                if self.size_topic_list == 0
                    && self.component_focused == Some(ComponentName::TopicList)
                {
                    self.focus_only(Some(ComponentName::Chat));
                }
            }
            Action::LoadForumTopics => {
                if let Some(topics) = self.components.get_mut(&ComponentName::TopicList) {
                    topics.update(action);
                }
            }
            Action::IncreaseChatListSize => {
                self.increase_chat_list_size();
            }
            Action::DecreaseChatListSize => {
                self.decrease_chat_list_size();
            }
            Action::IncreasePromptSize => {
                self.increase_size_prompt();
            }
            Action::DecreasePromptSize => {
                self.decrease_size_prompt();
            }
            Action::TryQuit => {
                if self.component_focused != Some(ComponentName::Prompt) {
                    self.action_tx
                        .as_ref()
                        .unwrap_or_else(|| panic!("Failed to get action_tx on CoreWindow"))
                        .send(Action::Quit)
                        .unwrap_or_else(|_| panic!("Failed to send action Quit from CoreWindow"));
                }
            }
            Action::HideChatWindowReply => {
                // The prompt stops replying too
                if let Some(prompt) = self.components.get_mut(&ComponentName::Prompt) {
                    prompt.update(action);
                }
                self.show_reply_message = false;
                self.app_context
                    .store()
                    .set_reply_message_i64(-1, String::new());
            }
            Action::ShowCommandGuide => {
                self.toggle_popup(ComponentName::CommandGuide, action, None);
            }
            Action::HideCommandGuide => self.close_popup(ComponentName::CommandGuide, None),
            Action::ShowMessageActionsFor(_) => {
                self.show_popup(ComponentName::MessageActions, action);
            }
            Action::HideMessageActions => {
                self.close_popup(ComponentName::MessageActions, Some(ComponentName::Chat));
            }
            Action::ShowLeaderMenu => {
                self.focus_before_leader = self.component_focused;
                self.show_popup(ComponentName::LeaderMenu, action);
            }
            Action::HideLeaderMenu => {
                let refocus = self.focus_before_leader.take();
                self.close_popup(ComponentName::LeaderMenu, refocus);
            }
            Action::ShowPicker(_) => self.show_popup(ComponentName::Picker, action),
            Action::HidePicker => {
                self.close_popup(ComponentName::Picker, Some(ComponentName::Chat))
            }
            // The chat lists act on their selected chat, other panes on the open one
            Action::ShowChatActions | Action::SelectedChat(_) | Action::ShowProfile => {
                if let Some(list @ (ComponentName::ChatList | ComponentName::ArchiveList)) =
                    self.component_focused
                {
                    if let Some(component) = self.components.get_mut(&list) {
                        component.update(action);
                    }
                    return;
                }
                let action_tx = self.app_context.action_tx();
                let chat_id = self.app_context.store().open_chat_id().as_i64();
                let _ = action_tx.send(match (chat_id, action) {
                    (0, _) => Action::StatusMessage("Open a chat first.".into()),
                    (_, Action::SelectedChat(command)) => chat_command_action(command, chat_id),
                    (_, Action::ShowProfile) => Action::ShowProfileFor(chat_id),
                    _ => Action::ShowChatActionsFor(chat_id, None),
                });
            }
            Action::ShowReport(..) => {
                self.focus_before_report = self.component_focused.filter(|c| !c.is_popup());
                self.show_popup(ComponentName::Report, action);
            }
            Action::HideReport => {
                let refocus = self.focus_before_report.take();
                self.close_popup(ComponentName::Report, refocus);
            }
            Action::ReportStepped(_) | Action::ReportFailed(_) => {
                if let Some(component) = self.components.get_mut(&ComponentName::Report) {
                    component.update(action);
                }
            }
            Action::ShowBlockedList => {
                self.focus_before_blocked_list = self.component_focused.filter(|c| !c.is_popup());
                self.show_popup(ComponentName::BlockedList, action);
            }
            Action::HideBlockedList => {
                let refocus = self.focus_before_blocked_list.take();
                self.close_popup(ComponentName::BlockedList, refocus);
            }
            Action::BlockedListLoaded(_) => {
                if let Some(component) = self.components.get_mut(&ComponentName::BlockedList) {
                    component.update(action);
                }
            }
            // The prompt has the caption, whatever is focused
            Action::UploadFile(_) => {
                if let Some(component) = self.components.get_mut(&ComponentName::Prompt) {
                    component.update(action);
                }
            }
            Action::ShowForward(..) => {
                self.focus_before_forward = self.component_focused.filter(|c| !c.is_popup());
                self.show_popup(ComponentName::Forward, action);
            }
            Action::HideForward => {
                let refocus = self.focus_before_forward.take();
                self.close_popup(ComponentName::Forward, refocus);
            }
            Action::ShowSessions => {
                self.focus_before_sessions = self.component_focused.filter(|c| !c.is_popup());
                self.show_popup(ComponentName::Sessions, action);
            }
            Action::HideSessions => {
                let refocus = self.focus_before_sessions.take();
                self.close_popup(ComponentName::Sessions, refocus);
            }
            Action::SessionsLoaded(_) => {
                if let Some(component) = self.components.get_mut(&ComponentName::Sessions) {
                    component.update(action);
                }
            }
            Action::ShowFindChat => {
                self.focus_before_find_chat = self.component_focused.filter(|c| !c.is_popup());
                self.show_popup(ComponentName::FindChat, action);
            }
            Action::HideFindChat => {
                let refocus = self.focus_before_find_chat.take();
                self.close_popup(ComponentName::FindChat, refocus);
            }
            Action::FindChatFailed(_) => {
                if let Some(component) = self.components.get_mut(&ComponentName::FindChat) {
                    component.update(action);
                }
            }
            // Opening the chat moves the focus to it
            Action::OpenChatById(chat_id) => {
                self.focus_before_find_chat = None;
                self.close_popup(ComponentName::FindChat, None);
                let store = self.app_context.store();
                let entry = ChatListEntry {
                    chat_id,
                    chat_name: store.name_from_chats(chat_id).unwrap_or_default(),
                    user: store.chat_user(chat_id),
                    ..ChatListEntry::default()
                };
                open_chat(&self.app_context, &entry);
            }
            Action::ShowMyProfile => {
                let me = self.app_context.store().me();
                let _ = self.app_context.action_tx().send(if me == 0 {
                    Action::StatusMessage("Your profile isn't known yet.".into())
                } else {
                    // Your user ID is also the ID of your chat with yourself
                    Action::ShowProfileFor(me)
                });
            }
            Action::ShowProfileFor(chat_id) => {
                if self.app_context.store().profile_source(chat_id).is_none() {
                    let _ = self
                        .app_context
                        .action_tx()
                        .send(Action::StatusMessage("No profile for this chat.".into()));
                    return;
                }
                self.focus_before_profile = self.component_focused.filter(|c| !c.is_popup());
                self.profile_chat = Some(chat_id);
                self.show_popup(ComponentName::Profile, action);
            }
            Action::ProfileLoaded(_) => {
                if let Some(component) = self.components.get_mut(&ComponentName::Profile) {
                    component.update(action);
                }
            }
            Action::HideProfile => {
                let refocus = self.focus_before_profile.take();
                self.profile_chat = None;
                self.close_popup(ComponentName::Profile, refocus);
            }
            Action::ShowEditProfile => {
                let me = self.app_context.store().me();
                let from_profile = self.is_popup_visible(ComponentName::Profile);
                let refusal = if me == 0 {
                    Some("Your profile isn't known yet.")
                } else if from_profile && self.profile_chat != Some(me) {
                    Some("Only your own profile can be edited.")
                } else {
                    None
                };
                if let Some(refusal) = refusal {
                    let _ = self
                        .app_context
                        .action_tx()
                        .send(Action::StatusMessage(refusal.into()));
                    return;
                }
                // Closing the profile goes back to where it was opened from
                self.focus_before_edit_profile = if from_profile {
                    self.profile_chat = None;
                    self.focus_before_profile.take()
                } else {
                    self.component_focused.filter(|c| !c.is_popup())
                };
                self.edit_profile_from_profile = from_profile;
                self.show_popup(ComponentName::EditProfile, action);
            }
            Action::HideEditProfile | Action::ProfileSaved => {
                let refocus = self.focus_before_edit_profile.take();
                let from_profile = std::mem::take(&mut self.edit_profile_from_profile);
                self.close_popup(ComponentName::EditProfile, refocus);
                if matches!(action, Action::ProfileSaved) {
                    let tx = self.app_context.action_tx();
                    let _ = tx.send(Action::StatusMessage("Profile saved.".into()));
                    if from_profile {
                        let _ = tx.send(Action::ShowMyProfile);
                    }
                }
            }
            Action::EditProfileLoaded | Action::EditProfileFailed(..) => {
                if let Some(component) = self.components.get_mut(&ComponentName::EditProfile) {
                    component.update(action);
                }
            }
            Action::ShowChatActionsFor(..) => {
                self.focus_before_chat_actions = self.component_focused.filter(|c| !c.is_popup());
                self.show_popup(ComponentName::ChatActions, action);
            }
            Action::HideChatActions => {
                let refocus = self.focus_before_chat_actions.take();
                self.close_popup(ComponentName::ChatActions, refocus);
            }
            Action::ShowStickerPicker => {
                if self.app_context.store().open_chat_id().is_none() {
                    let _ = self.app_context.action_tx().send(Action::StatusMessage(
                        "Open a chat before sending a sticker.".into(),
                    ));
                    return;
                }
                self.focus_before_stickers = self.component_focused.filter(|c| !c.is_popup());
                self.show_popup(ComponentName::StickerPicker, action);
            }
            Action::HideStickerPicker => {
                let refocus = self
                    .focus_before_stickers
                    .take()
                    .or(Some(ComponentName::Prompt));
                self.close_popup(ComponentName::StickerPicker, refocus);
            }
            // Answers to the picker's requests, wherever the focus went meanwhile
            Action::StickerSetsLoaded(_) | Action::StickerSetLoaded(_) => {
                if let Some(picker) = self.components.get_mut(&ComponentName::StickerPicker) {
                    picker.update(action);
                }
            }
            Action::RunMessageCommand(..) => {
                // From the message actions menu, which may still be focused
                if let Some(chat) = self.components.get_mut(&ComponentName::Chat) {
                    chat.update(action);
                }
            }
            #[cfg(feature = "media-preview")]
            Action::PreviewDecoded(_) => {
                // Each keeps only the previews it asked for
                for name in [ComponentName::Chat, ComponentName::StickerPicker] {
                    if let Some(component) = self.components.get_mut(&name) {
                        component.update(action.clone());
                    }
                }
            }
            Action::ShowFileUploadExplorer => {
                self.toggle_popup(
                    ComponentName::FileUploadExplorer,
                    action,
                    Some(ComponentName::Chat),
                );
            }
            Action::HideFileUploadExplorer => {
                self.close_popup(ComponentName::FileUploadExplorer, Some(ComponentName::Chat));
            }
            Action::ShowPinnedMessagesPopup => {
                if self
                    .app_context
                    .store()
                    .open_chat_pinned_snapshot()
                    .is_empty()
                {
                    return;
                }
                self.show_popup(ComponentName::PinnedMessagesPopup, action);
            }
            Action::HidePinnedMessagesPopup => {
                self.close_popup(
                    ComponentName::PinnedMessagesPopup,
                    Some(ComponentName::Chat),
                );
            }
            Action::Key(key_code, modifiers) => {
                // Note: Popup components are focusable and use keymaps.
                // Key events are routed to the focused component via the keymap system in run.rs.
                // Send key events to focused component
                if let Some(focused) = self.component_focused {
                    self.components
                        .get_mut(&focused)
                        .unwrap_or_else(|| panic!("Failed to get component: {focused}"))
                        .update(Action::Key(key_code, modifiers));
                }
                // Don't pass action to focused component again below
            }
            Action::ChatListSearch => {
                // If search overlay is open, close it first then focus ChatList
                self.hide_popup(ComponentName::SearchOverlay);
                // Activate ChatList search when ChatList is focused, nothing focused, or Chat focused
                let should_activate_search = match self.component_focused {
                    None => true,
                    Some(ComponentName::ChatList) => true,
                    Some(ComponentName::Chat) => true,
                    Some(ComponentName::SearchOverlay) => true, // switching from overlay to ChatList search
                    Some(ComponentName::Prompt) => false,
                    _ => false,
                };

                if should_activate_search {
                    // Focus ChatList and activate search mode
                    self.focus_only(Some(ComponentName::ChatList));
                    if let Some(component) = self.components.get_mut(&ComponentName::ChatList) {
                        component.update(action.clone());
                    }
                }
            }
            Action::ChatWindowSearch => {
                // Show server-side search overlay and focus it
                self.show_popup(ComponentName::SearchOverlay, Action::ShowSearchOverlay);
            }
            Action::CloseSearchOverlay => {
                self.hide_popup(ComponentName::SearchOverlay);
                // Refocus Chat after closing search
                self.focus_only(Some(ComponentName::Chat));
            }
            Action::SearchResults(_) => {
                // Propagate to SearchOverlay when visible
                if self.is_popup_visible(ComponentName::SearchOverlay)
                    && let Some(component) = self.components.get_mut(&ComponentName::SearchOverlay)
                {
                    component.update(action.clone());
                }
            }
            Action::ChatListSortWithString(_) => {
                // Propagate to ChatList component even if not focused
                if let Some(component) = self.components.get_mut(&ComponentName::ChatList) {
                    component.update(action.clone());
                }
                // Also update focused component if it's not ChatList
                if let Some(focused) = self.component_focused
                    && focused != ComponentName::ChatList
                {
                    self.components
                        .get_mut(&focused)
                        .unwrap_or_else(|| panic!("Failed to get component: {focused}"))
                        .update(action);
                }
            }
            Action::ChatWindowSortWithString(_) => {
                // Propagate to Chat component even if not focused
                if let Some(component) = self.components.get_mut(&ComponentName::Chat) {
                    component.update(action.clone());
                }
                // Also update focused component if it's not Chat
                if let Some(focused) = self.component_focused
                    && focused != ComponentName::Chat
                {
                    self.components
                        .get_mut(&focused)
                        .unwrap_or_else(|| panic!("Failed to get component: {focused}"))
                        .update(action);
                }
            }
            Action::ChatListRestoreSort => {
                // Propagate to ChatList component even if not focused
                if let Some(component) = self.components.get_mut(&ComponentName::ChatList) {
                    component.update(action.clone());
                }
                // Also update focused component if it's not ChatList
                if let Some(focused) = self.component_focused
                    && focused != ComponentName::ChatList
                {
                    self.components
                        .get_mut(&focused)
                        .unwrap_or_else(|| panic!("Failed to get component: {focused}"))
                        .update(action);
                }
            }
            Action::ChatWindowRestoreSort => {
                // Propagate to Chat component even if not focused
                if let Some(component) = self.components.get_mut(&ComponentName::Chat) {
                    component.update(action.clone());
                }
                // Also update focused component if it's not Chat
                if let Some(focused) = self.component_focused
                    && focused != ComponentName::Chat
                {
                    self.components
                        .get_mut(&focused)
                        .unwrap_or_else(|| panic!("Failed to get component: {focused}"))
                        .update(action);
                }
            }
            Action::LoadPinnedMessages => {
                if let Some(component) = self.components.get_mut(&ComponentName::Chat) {
                    component.update(action.clone());
                }
                if let Some(focused) = self.component_focused
                    && focused != ComponentName::Chat
                {
                    self.components
                        .get_mut(&focused)
                        .unwrap_or_else(|| panic!("Failed to get component: {focused}"))
                        .update(action);
                }
            }
            Action::EditMessage(..) | Action::ReplyMessage(..) => {
                // Always dispatch to Prompt so the message is loaded regardless of focus order.
                if let Some(component) = self.components.get_mut(&ComponentName::Prompt) {
                    component.update(action);
                }
            }
            Action::LoadChats(..) | Action::ChatHistoryAppended | Action::Resize(..) => {
                // Always forward to ChatList so it can rebuild_visible_chats (populates visible_chats).
                if let Some(component) = self.components.get_mut(&ComponentName::ChatList) {
                    component.update(action.clone());
                }
                if let Some(focused) = self.component_focused
                    && focused != ComponentName::ChatList
                    && let Some(component) = self.components.get_mut(&focused)
                {
                    component.update(action);
                }
            }
            _ => {
                if let Some(focused) = self.component_focused {
                    self.components
                        .get_mut(&focused)
                        .unwrap_or_else(|| panic!("Failed to get component: {focused}"))
                        .update(action);
                }
            }
        }
    }

    fn draw(&mut self, frame: &mut ratatui::Frame<'_>, area: Rect) -> io::Result<()> {
        self.fit_side_panes();
        let size_archive_list = self.side_pane_width(ComponentName::ArchiveList);
        let size_topic_list = self.side_pane_width(ComponentName::TopicList);
        let core_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(self.size_chat_list),
                Constraint::Percentage(size_archive_list),
                Constraint::Percentage(size_topic_list),
                Constraint::Fill(1),
            ])
            .split(area);

        let sub_core_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Fill(1),
                {
                    if self.show_reply_message {
                        Constraint::Length(self.size_message_reply)
                    } else {
                        Constraint::Length(0)
                    }
                },
                Constraint::Length(self.size_prompt + self.prompt_extra_rows),
            ])
            .split(core_layout[3]);

        // Store areas for click-to-focus (chat list, archive, topic list, chat window, prompt)
        self.last_focusable_areas
            .insert(ComponentName::ChatList, core_layout[0]);
        self.last_focusable_areas
            .insert(ComponentName::ArchiveList, core_layout[1]);
        self.last_focusable_areas
            .insert(ComponentName::TopicList, core_layout[2]);
        self.last_focusable_areas
            .insert(ComponentName::Chat, sub_core_layout[0]);
        self.last_focusable_areas
            .insert(ComponentName::Prompt, sub_core_layout[2]);

        self.components
            .get_mut(&ComponentName::ChatList)
            .unwrap_or_else(|| panic!("Failed to get component: {}", ComponentName::ChatList))
            .draw(frame, core_layout[0])?;

        if size_archive_list > 0 {
            self.components
                .get_mut(&ComponentName::ArchiveList)
                .unwrap_or_else(|| {
                    panic!("Failed to get component: {}", ComponentName::ArchiveList)
                })
                .draw(frame, core_layout[1])?;
        }

        if size_topic_list > 0 {
            self.components
                .get_mut(&ComponentName::TopicList)
                .unwrap_or_else(|| panic!("Failed to get component: {}", ComponentName::TopicList))
                .draw(frame, core_layout[2])?;
        }

        self.components
            .get_mut(&ComponentName::Chat)
            .unwrap_or_else(|| panic!("Failed to get component: {}", ComponentName::Chat))
            .draw(frame, sub_core_layout[0])?;

        if self.show_reply_message {
            self.components
                .get_mut(&ComponentName::ReplyMessage)
                .unwrap_or_else(|| {
                    panic!("Failed to get component: {}", ComponentName::ReplyMessage)
                })
                .draw(frame, sub_core_layout[1])?;
        }
        self.components
            .get_mut(&ComponentName::Prompt)
            .unwrap_or_else(|| panic!("Failed to get component: {}", ComponentName::Prompt))
            .draw(frame, sub_core_layout[2])?;

        // Draw the popup on top of everything
        if let Some(popup) = self.active_popup {
            self.components
                .get_mut(&popup)
                .unwrap_or_else(|| panic!("Failed to get component: {popup}"))
                .draw(frame, area)?;
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/core_window.rs"]
mod tests;
