use std::sync::{
    Arc, Mutex, MutexGuard,
    atomic::{AtomicBool, AtomicU8, Ordering},
};

use libtgt::store::Store;
use ratatui::style::Style;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{
    app::action::Action,
    config::theme::ThemeSource,
    settings::{App, Dirs, Keymap, Settings, Theme},
    ui::component_name::ComponentName,
};

/// Generates a getter for a theme style: `$map` section, `$attr_name` key.
/// Returns the default style when themes are disabled or the key is missing.
macro_rules! theme_style_generate {
    ($fn_name: ident, $map: ident, $attr_name: ident) => {
        #[inline]
        pub fn $fn_name(&self) -> Style {
            if self.app.theme_enable {
                self.theme()
                    .$map
                    .get(stringify!($attr_name))
                    .copied()
                    .unwrap_or_default()
            } else {
                Style::default()
            }
        }
    };
}

/// State shared between the run loop and the UI components.
#[derive(Debug)]
pub struct AppContext {
    app: App,
    keymap: Keymap,
    dirs: Dirs,
    /// Themes the UI can switch to.
    themes: Vec<(String, ThemeSource)>,
    /// The active theme; replaced when the user switches themes.
    theme: Mutex<Theme>,
    /// Actions from components and background tasks to the run loop.
    action_tx: UnboundedSender<Action>,
    action_rx: Mutex<UnboundedReceiver<Action>>,
    quit: AtomicBool,
    /// When true, the run loop draws once and clears the flag.
    needs_render: AtomicBool,
    store: Arc<Store>,
    /// The focused component, for context-aware keymap lookup in the run loop.
    /// Encoded as: 0 = None, 1-14 = ComponentName variants.
    focused_component: AtomicU8,
    /// Result of background photo decode; consumed by PhotoViewer on PhotoDecoded(i64).
    pending_photo_decoded: Mutex<Option<(i64, Result<image::DynamicImage, String>)>>,

    #[cfg(feature = "audio")]
    /// Current voice/audio playback state. Updated by the playback thread.
    voice_playback_state: Mutex<crate::audio::VoicePlaybackState>,
    #[cfg(feature = "audio")]
    /// Sender to the voice playback thread; None if audio output is unavailable.
    voice_playback_tx: Mutex<Option<std::sync::mpsc::Sender<crate::audio::VoicePlaybackCommand>>>,
}

impl AppContext {
    pub fn new(settings: &Settings, store: Arc<Store>) -> Self {
        let (action_tx, action_rx) = tokio::sync::mpsc::unbounded_channel::<Action>();
        Self {
            app: settings.app.clone(),
            keymap: settings.keymap.clone(),
            dirs: settings.dirs.clone(),
            themes: settings.themes.clone(),
            theme: Mutex::new(settings.theme.clone()),
            action_tx,
            action_rx: Mutex::new(action_rx),
            quit: AtomicBool::new(false),
            needs_render: AtomicBool::new(false),
            store,
            focused_component: AtomicU8::new(0),
            pending_photo_decoded: Mutex::new(None),
            #[cfg(feature = "audio")]
            voice_playback_state: Mutex::new(crate::audio::VoicePlaybackState::default()),
            #[cfg(feature = "audio")]
            voice_playback_tx: Mutex::new(None),
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

    pub fn themes(&self) -> &[(String, ThemeSource)] {
        &self.themes
    }

    pub fn theme(&self) -> MutexGuard<'_, Theme> {
        self.theme.lock().unwrap()
    }

    pub fn set_theme(&self, theme: Theme) {
        *self.theme() = theme;
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

    /// Set decoded photo from background thread; run loop calls this before sending PhotoDecoded(i64).
    pub fn set_pending_photo_decoded(
        &self,
        message_id: i64,
        result: Result<image::DynamicImage, String>,
    ) {
        *self.pending_photo_decoded.lock().unwrap() = Some((message_id, result));
    }

    /// Take decoded photo for PhotoViewer; returns None if message_id does not match or slot empty.
    pub fn take_pending_photo_decoded(
        &self,
        message_id: i64,
    ) -> Option<Result<image::DynamicImage, String>> {
        let mut guard = self.pending_photo_decoded.lock().unwrap();
        let taken = guard.take()?;
        if taken.0 == message_id {
            Some(taken.1)
        } else {
            *guard = Some(taken);
            None
        }
    }

    #[cfg(feature = "audio")]
    /// Voice playback state (for status bar counter). Lock and read.
    pub fn voice_playback_state(&self) -> MutexGuard<'_, crate::audio::VoicePlaybackState> {
        self.voice_playback_state.lock().unwrap()
    }

    #[cfg(feature = "audio")]
    /// Set the command sender for the voice playback thread (called from run when thread is spawned).
    pub fn set_voice_playback_tx(
        &self,
        tx: std::sync::mpsc::Sender<crate::audio::VoicePlaybackCommand>,
    ) {
        *self.voice_playback_tx.lock().unwrap() = Some(tx);
    }

    #[cfg(feature = "audio")]
    /// Send a command to the voice playback thread. Returns false if channel disconnected (thread exited).
    pub fn voice_playback_send(&self, cmd: crate::audio::VoicePlaybackCommand) -> bool {
        if let Some(ref tx) = *self.voice_playback_tx.lock().unwrap() {
            tx.send(cmd).is_ok()
        } else {
            false
        }
    }

    /// The Telegram store shared with the client.
    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    /// Encodes `Option<ComponentName>` to a `u8` for atomic storage.
    /// Encoding: 0 = None, 1-14 = ComponentName variants.
    #[inline]
    fn encode_component(component: Option<ComponentName>) -> u8 {
        match component {
            None => 0,
            Some(ComponentName::CoreWindow) => 1,
            Some(ComponentName::ChatList) => 2,
            Some(ComponentName::Chat) => 3,
            Some(ComponentName::Prompt) => 4,
            Some(ComponentName::ReplyMessage) => 5,
            Some(ComponentName::TitleBar) => 6,
            Some(ComponentName::StatusBar) => 7,
            Some(ComponentName::CommandGuide) => 8,
            Some(ComponentName::ThemeSelector) => 9,
            Some(ComponentName::SearchOverlay) => 10,
            Some(ComponentName::PhotoViewer) => 11,
            Some(ComponentName::FileUploadExplorer) => 12,
            Some(ComponentName::FileDownloadExplorer) => 13,
            Some(ComponentName::PinnedMessagesPopup) => 14,
        }
    }

    /// Decodes a `u8` to `Option<ComponentName>` from atomic storage.
    #[inline]
    fn decode_component(encoded: u8) -> Option<ComponentName> {
        match encoded {
            0 => None,
            1 => Some(ComponentName::CoreWindow),
            2 => Some(ComponentName::ChatList),
            3 => Some(ComponentName::Chat),
            4 => Some(ComponentName::Prompt),
            5 => Some(ComponentName::ReplyMessage),
            6 => Some(ComponentName::TitleBar),
            7 => Some(ComponentName::StatusBar),
            8 => Some(ComponentName::CommandGuide),
            9 => Some(ComponentName::ThemeSelector),
            10 => Some(ComponentName::SearchOverlay),
            11 => Some(ComponentName::PhotoViewer),
            12 => Some(ComponentName::FileUploadExplorer),
            13 => Some(ComponentName::FileDownloadExplorer),
            14 => Some(ComponentName::PinnedMessagesPopup),
            _ => None, // Invalid encoding, treat as None
        }
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

    // ===== COMMON ======
    theme_style_generate!(
        style_border_component_focused,
        common,
        border_component_focused
    );

    theme_style_generate!(style_item_selected, common, item_selected);
    theme_style_generate!(style_item_reply_target, common, item_reply_target);
    theme_style_generate!(style_timestamp, common, timestamp);

    // ===== CHAT LIST =====
    theme_style_generate!(style_chat_list, chat_list, self);
    theme_style_generate!(style_chat_list_item_selected, chat_list, item_selected);
    theme_style_generate!(style_chat_list_item_chat_name, chat_list, item_chat_name);
    theme_style_generate!(
        style_chat_list_item_message_content,
        chat_list,
        item_message_content
    );
    theme_style_generate!(
        style_chat_list_item_unread_counter,
        chat_list,
        item_unread_counter
    );

    // ===== CHAT =====
    theme_style_generate!(style_chat, chat, self);
    theme_style_generate!(style_chat_chat_name, chat, chat_name);
    theme_style_generate!(style_chat_message_myself_name, chat, message_myself_name);
    theme_style_generate!(style_chat_message_other_name, chat, message_other_name);
    theme_style_generate!(
        style_chat_message_myself_content,
        chat,
        message_myself_content
    );
    theme_style_generate!(
        style_chat_message_other_content,
        chat,
        message_other_content
    );
    theme_style_generate!(style_chat_message_reply_text, chat, message_reply_text);
    theme_style_generate!(
        style_chat_message_myself_reply_name,
        chat,
        message_myself_reply_name
    );
    theme_style_generate!(
        style_chat_message_myself_reply_content,
        chat,
        message_myself_reply_content
    );
    theme_style_generate!(
        style_chat_message_other_reply_name,
        chat,
        message_other_reply_name
    );
    theme_style_generate!(
        style_chat_message_other_reply_content,
        chat,
        message_other_reply_content
    );

    // ===== PROMPT =====
    theme_style_generate!(style_prompt, prompt, self);
    theme_style_generate!(style_prompt_message_text, prompt, message_text);
    theme_style_generate!(
        style_prompt_message_text_selected,
        prompt,
        message_text_selected
    );
    theme_style_generate!(
        style_prompt_message_preview_text,
        prompt,
        message_preview_text
    );

    // ===== REPLY MESSAGE =====
    theme_style_generate!(style_reply_message, reply_message, self);
    theme_style_generate!(
        style_reply_message_message_text,
        reply_message,
        message_text
    );

    // ===== STATUS BAR =====
    theme_style_generate!(style_status_bar, status_bar, self);
    theme_style_generate!(style_status_bar_size_info_text, status_bar, size_info_text);
    theme_style_generate!(
        style_status_bar_size_info_numbers,
        status_bar,
        size_info_numbers
    );
    theme_style_generate!(style_status_bar_press_key_text, status_bar, press_key_text);
    theme_style_generate!(style_status_bar_press_key_key, status_bar, press_key_key);
    theme_style_generate!(
        style_status_bar_message_quit_text,
        status_bar,
        message_quit_text
    );
    theme_style_generate!(
        style_status_bar_message_quit_key,
        status_bar,
        message_quit_key
    );
    theme_style_generate!(style_status_bar_open_chat_text, status_bar, open_chat_text);
    theme_style_generate!(style_status_bar_open_chat_name, status_bar, open_chat_name);

    // ===== TITLE BAR =====
    theme_style_generate!(style_title_bar, title_bar, self);
    theme_style_generate!(style_title_bar_title1, title_bar, title1);
    theme_style_generate!(style_title_bar_title2, title_bar, title2);
    theme_style_generate!(style_title_bar_title3, title_bar, title3);
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
