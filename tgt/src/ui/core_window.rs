use crate::{
    app::action::Action,
    app::context::AppContext,
    app::error::AppError,
    app::event::Event,
    settings::ActionBinding,
    ui::component_name::ComponentName,
    ui::{MAX_CHAT_LIST_SIZE, MAX_PROMPT_SIZE, MIN_CHAT_LIST_SIZE, MIN_PROMPT_SIZE},
    ui::{
        chat_list_window::ChatListWindow,
        chat_window::ChatWindow,
        command_guide::CommandGuide,
        component_traits::{Component, HandleFocus},
        file_upload_explorer::FileUploadExplorer,
        leader_menu::LeaderMenu,
        message_actions::MessageActions,
        picker::Picker,
        pinned_messages_popup::PinnedMessagesPopup,
        prompt_window::PromptWindow,
        search_overlay::SearchOverlay,
    },
};
use crossterm::event::{MouseButton, MouseEventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use std::{collections::HashMap, io, sync::Arc};
use tokio::sync::mpsc::UnboundedSender;

use super::reply_message::ReplyMessage;

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
    /// The size of the message reply component.
    size_message_reply: u16,
    /// The size of the chat list component.
    size_chat_list: u16,
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
        let size_chat_list = 20;
        let small_area = false;
        let component_focused = None;
        let focused = true;
        let show_reply_message = false;
        let last_focusable_areas = HashMap::new();

        CoreWindow {
            app_context,
            name,
            action_tx,
            components,
            size_chat_list,
            size_prompt,
            prompt_extra_rows: 0,
            focus_before_leader: None,
            size_message_reply,
            small_area,
            component_focused,
            focused,
            show_reply_message,
            active_popup: None,
            last_focusable_areas,
        }
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
        self.size_chat_list = if self.size_chat_list == 0 { 20 } else { 0 };
    }
    /// Increase the size of the chat list component.
    pub fn increase_chat_list_size(&mut self) {
        if self.size_chat_list == MAX_CHAT_LIST_SIZE {
            return;
        }
        self.size_chat_list += 1;
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
        if self.size_chat_list == MIN_CHAT_LIST_SIZE {
            return;
        }
        self.size_chat_list -= 1;
    }
    /// Decrease the size of the chat list component.
    pub fn decrease_size_prompt(&mut self) {
        if self.size_prompt == MIN_PROMPT_SIZE {
            return;
        }
        self.size_prompt -= 1;
    }

    /// Panes in the order Left/Right move through; a hidden chat list is skipped.
    fn panes(&self) -> Vec<ComponentName> {
        let mut panes = vec![ComponentName::Chat, ComponentName::Prompt];
        if self.size_chat_list > 0 {
            panes.insert(0, ComponentName::ChatList);
        }
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
    /// The chat and the prompt share the right side, so they grow together.
    fn resize_focused_width(&mut self, wider: bool) {
        if self.size_chat_list == 0 {
            return;
        }
        match (self.component_focused, wider) {
            (Some(ComponentName::ChatList), true)
            | (Some(ComponentName::Chat | ComponentName::Prompt), false) => {
                self.increase_chat_list_size();
            }
            (Some(ComponentName::ChatList), false)
            | (Some(ComponentName::Chat | ComponentName::Prompt), true) => {
                self.decrease_chat_list_size();
            }
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
            ComponentName::LeaderMenu => Action::HideLeaderMenu,
            _ => return None,
        })
    }

    fn is_popup_visible(&self, popup: ComponentName) -> bool {
        self.active_popup == Some(popup)
    }

    /// Focuses `name` (or nothing) and unfocuses every other component.
    fn focus_only(&mut self, name: Option<ComponentName>) {
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
        // Forward mouse events to the focused component (scroll wheel, click, etc.)
        if let Event::Mouse(mouse) = ev {
            // Left click: focus the section under the cursor (chat list, chat window, prompt)
            if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
                let col = mouse.column;
                let row = mouse.row;
                for name in [
                    ComponentName::ChatList,
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
            Action::FocusComponent(component_name) => {
                // If the previously focused component was a popup, hide it automatically
                if let Some(prev_focused) = self.component_focused
                    && prev_focused.is_popup()
                    && prev_focused != component_name
                {
                    self.hide_popup(prev_focused);
                }

                self.focus_only(Some(component_name));
            }
            Action::UnfocusComponent => {
                self.show_reply_message = false;
                if let Some(popup) = self.active_popup {
                    self.hide_popup(popup);
                }
                // Leaving the prompt goes back to the messages, like concord
                let back_to_chat = self.component_focused == Some(ComponentName::Prompt);
                self.focus_only(back_to_chat.then_some(ComponentName::Chat));
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
            Action::RunMessageCommand(..) => {
                // From the message actions menu, which may still be focused
                if let Some(chat) = self.components.get_mut(&ComponentName::Chat) {
                    chat.update(action);
                }
            }
            #[cfg(feature = "media-preview")]
            Action::PreviewDecoded(_) => {
                if let Some(chat) = self.components.get_mut(&ComponentName::Chat) {
                    chat.update(action);
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
        let core_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(self.size_chat_list),
                Constraint::Percentage(100 - self.size_chat_list),
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
            .split(core_layout[1]);

        // Store areas for click-to-focus (chat list, chat window, prompt)
        self.last_focusable_areas
            .insert(ComponentName::ChatList, core_layout[0]);
        self.last_focusable_areas
            .insert(ComponentName::Chat, sub_core_layout[0]);
        self.last_focusable_areas
            .insert(ComponentName::Prompt, sub_core_layout[2]);

        self.components
            .get_mut(&ComponentName::ChatList)
            .unwrap_or_else(|| panic!("Failed to get component: {}", ComponentName::ChatList))
            .draw(frame, core_layout[0])?;

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
