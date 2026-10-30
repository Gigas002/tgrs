use std::sync::{
    Arc, Mutex, MutexGuard,
    atomic::{AtomicBool, AtomicU8, Ordering},
};

use libtgt::store::Store;
use ratatui::style::Style;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{
    app::action::Action,
    settings::{App, Dirs, HighlightGroup, Keymap, Settings, Theme},
    ui::component_name::ComponentName,
};

/// Generates a getter for the style of a [`HighlightGroup`].
macro_rules! theme_style_generate {
    ($fn_name: ident, $group: ident) => {
        #[inline]
        pub fn $fn_name(&self) -> Style {
            self.theme.style(HighlightGroup::$group)
        }
    };
}

/// State shared between the run loop and the UI components.
#[derive(Debug)]
pub struct AppContext {
    app: App,
    keymap: Keymap,
    dirs: Dirs,
    theme: Theme,
    /// Actions from components and background tasks to the run loop.
    action_tx: UnboundedSender<Action>,
    action_rx: Mutex<UnboundedReceiver<Action>>,
    quit: AtomicBool,
    /// When true, the run loop draws once and clears the flag.
    needs_render: AtomicBool,
    store: Arc<Store>,
    /// The focused component, for context-aware keymap lookup in the run loop.
    /// Encoded as: 0 = None, n = `ComponentName::ALL[n - 1]`.
    focused_component: AtomicU8,
}

impl AppContext {
    pub fn new(settings: &Settings, store: Arc<Store>) -> Self {
        let (action_tx, action_rx) = tokio::sync::mpsc::unbounded_channel::<Action>();
        Self {
            app: settings.app.clone(),
            keymap: settings.keymap.clone(),
            dirs: settings.dirs.clone(),
            theme: settings.theme.clone(),
            action_tx,
            action_rx: Mutex::new(action_rx),
            quit: AtomicBool::new(false),
            needs_render: AtomicBool::new(false),
            store,
            focused_component: AtomicU8::new(0),
        }
    }

    pub fn app_config(&self) -> &App {
        &self.app
    }

    pub fn keymap_config(&self) -> &Keymap {
        &self.keymap
    }

    pub fn dirs(&self) -> &Dirs {
        &self.dirs
    }

    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Get the action receiver.
    /// This function takes the lock on the action receiver and returns the action
    /// receiver.
    /// The action receiver is a shared resource and is protected by a mutex.
    ///
    /// # Returns
    /// * `MutexGuard<'_, UnboundedReceiver<Action>>` - The action receiver.
    pub fn action_rx(&self) -> MutexGuard<'_, UnboundedReceiver<Action>> {
        self.action_rx.lock().unwrap()
    }
    pub fn action_tx(&self) -> &UnboundedSender<Action> {
        &self.action_tx
    }
    /// Get the quit flag.
    /// This function returns the value of the quit flag.
    /// The quit flag is a shared resource and is protected by an atomic boolean.
    ///
    /// # Returns
    /// * `bool` - The value of the quit flag.
    pub fn quit_acquire(&self) -> bool {
        self.quit.load(Ordering::Acquire)
    }
    /// Set the quit flag.
    /// This function sets the value of the quit flag.
    /// The quit flag is a shared resource and is protected by an atomic boolean.
    pub fn quit_store(&self, value: bool) {
        self.quit.store(value, Ordering::Release);
    }

    // ----- Render-on-demand (needs_render) -----

    /// Returns whether a render is needed. Used by the main loop to decide
    /// whether to call `terminal.draw()`.
    #[inline]
    pub fn needs_render(&self) -> bool {
        self.needs_render.load(Ordering::Acquire)
    }

    /// Alias for `needs_render()`. Returns true when the UI should be drawn.
    #[inline]
    pub fn should_render(&self) -> bool {
        self.needs_render()
    }

    /// Sets the render-needed flag. Idempotent; safe to call multiple times.
    pub fn set_needs_render(&self, value: bool) {
        self.needs_render.store(value, Ordering::Release);
    }

    /// Marks the UI as dirty so that the next render pass will redraw.
    /// Prefer this over sending `Action::Render` from components.
    #[inline]
    pub fn mark_dirty(&self) {
        self.set_needs_render(true);
    }

    /// Clears the render flag and returns the previous value.
    /// The main loop uses this after drawing so we only draw when dirty.
    #[inline]
    pub fn clear_render_flag(&self) -> bool {
        self.needs_render.swap(false, Ordering::SeqCst)
    }

    /// The Telegram store shared with the client.
    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    /// Encodes `Option<ComponentName>` to a `u8` for atomic storage.
    #[inline]
    fn encode_component(component: Option<ComponentName>) -> u8 {
        component
            .and_then(|component| ComponentName::ALL.iter().position(|c| *c == component))
            .map_or(0, |index| index as u8 + 1)
    }

    /// Decodes a `u8` to `Option<ComponentName>` from atomic storage.
    #[inline]
    fn decode_component(encoded: u8) -> Option<ComponentName> {
        (encoded as usize)
            .checked_sub(1)
            .and_then(|index| ComponentName::ALL.get(index).copied())
    }

    /// Returns the currently focused UI component for context-aware keymap lookup.
    /// Lock-free read using atomic operations.
    #[inline]
    pub fn focused_component(&self) -> Option<ComponentName> {
        Self::decode_component(self.focused_component.load(Ordering::Acquire))
    }

    /// Sets the currently focused UI component. Called by CoreWindow when focus changes.
    /// Lock-free write using atomic operations.
    #[inline]
    pub fn set_focused_component(&self, component: Option<ComponentName>) {
        self.focused_component
            .store(Self::encode_component(component), Ordering::Release);
    }

    /// The style of `group` in the theme.
    pub fn style(&self, group: HighlightGroup) -> Style {
        self.theme.style(group)
    }

    // Component styles, by the Highlight Group each one uses
    theme_style_generate!(style_border_component_focused, FocusedPaneBorder);
    theme_style_generate!(style_item_selected, SelectedRow);
    theme_style_generate!(style_item_reply_target, ReplyTargetBorder);
    theme_style_generate!(style_timestamp, Timestamp);

    theme_style_generate!(style_chat_list, Normal);
    theme_style_generate!(style_chat_list_item_selected, SelectedRow);
    theme_style_generate!(style_chat_list_item_chat_name, Strong);
    theme_style_generate!(style_chat_list_item_message_content, ChatPreview);
    theme_style_generate!(style_chat_list_item_unread_counter, NotificationBadge);

    theme_style_generate!(style_chat, Normal);
    theme_style_generate!(style_chat_chat_name, PaneTitle);
    theme_style_generate!(style_chat_message_myself_name, SelfMessageAuthor);
    theme_style_generate!(style_chat_message_other_name, MessageAuthor);
    theme_style_generate!(style_chat_message_myself_content, MessageBody);
    theme_style_generate!(style_chat_message_other_content, MessageBody);
    theme_style_generate!(style_chat_message_reply_text, MessageSecondary);
    theme_style_generate!(style_chat_message_myself_reply_name, MessageSecondary);
    theme_style_generate!(style_chat_message_myself_reply_content, MessageSecondary);
    theme_style_generate!(style_chat_message_other_reply_name, MessageSecondary);
    theme_style_generate!(style_chat_message_other_reply_content, MessageSecondary);

    theme_style_generate!(style_prompt, Normal);
    theme_style_generate!(style_prompt_message_text, MessageBody);
    theme_style_generate!(style_prompt_message_text_selected, SelectedRow);
    theme_style_generate!(style_prompt_message_preview_text, Placeholder);

    theme_style_generate!(style_reply_message, ReplyTargetBorder);
    theme_style_generate!(style_reply_message_message_text, MessageSecondary);

    theme_style_generate!(style_status_bar, Normal);
    theme_style_generate!(style_status_bar_size_info_text, HeaderLabel);
    theme_style_generate!(style_status_bar_size_info_numbers, Normal);
    theme_style_generate!(style_status_bar_press_key_text, HeaderLabel);
    theme_style_generate!(style_status_bar_press_key_key, Shortcut);
    theme_style_generate!(style_status_bar_message_quit_text, HeaderLabel);
    theme_style_generate!(style_status_bar_message_quit_key, Shortcut);
    theme_style_generate!(style_status_bar_open_chat_text, HeaderLabel);
    theme_style_generate!(style_status_bar_open_chat_name, HeaderTitle);

    theme_style_generate!(style_title_bar, Normal);
    theme_style_generate!(style_title_bar_title1, HeaderTitle);
    theme_style_generate!(style_title_bar_title2, HeaderTitle);
    theme_style_generate!(style_title_bar_title3, HeaderTitle);
}

#[cfg(test)]
impl AppContext {
    /// A context with default settings and an empty store.
    pub(crate) fn for_tests() -> Arc<Self> {
        Arc::new(Self::new(
            &Settings::for_tests(),
            Arc::new(Store::default()),
        ))
    }
}
