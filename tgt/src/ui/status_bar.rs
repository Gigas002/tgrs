use {
    crate::{
        app::action::Action,
        app::context::AppContext,
        settings::HighlightGroup,
        ui::component_name::ComponentName,
        ui::component_traits::{Component, HandleFocus},
    },
    ratatui::{
        layout::Rect,
        text::{Line, Span},
        widgets::Paragraph,
    },
    std::sync::Arc,
    std::time::Instant,
    tokio::sync::mpsc::UnboundedSender,
};

/// `StatusBar` is a struct that represents a status bar.
/// It is responsible for managing the layout and rendering of the status bar.
pub struct StatusBar {
    /// The application configuration.
    app_context: Arc<AppContext>,
    /// The name of the `StatusBar`.
    name: String,
    /// An unbounded sender that send action for processing.
    command_tx: Option<UnboundedSender<Action>>,
    /// Indicates whether the `StatusBar` is focused or not.
    focused: bool,
    /// Optional short status message; cleared after STATUS_MESSAGE_DURATION or on next key press.
    status_message: Option<String>,
    /// When the status message was set (for 5s auto-clear).
    status_message_set_at: Option<Instant>,
}

/// How long a status message is shown before the hints come back.
const STATUS_MESSAGE_DURATION_SECS: u64 = 5;
/// Implementation of `StatusBar` struct.
impl StatusBar {
    /// Create a new instance of the `StatusBar` struct.
    ///
    /// # Arguments
    /// * `app_context` - An Arc wrapped AppContext struct.
    ///
    /// # Returns
    /// * `Self` - The new instance of the `StatusBar` struct.
    pub fn new(app_context: Arc<AppContext>) -> Self {
        let command_tx = None;
        let name = "".to_string();
        let focused = false;
        let status_message = None;
        let status_message_set_at = None;

        StatusBar {
            app_context,
            command_tx,
            name,
            focused,
            status_message,
            status_message_set_at,
        }
    }
    /// Set the name of the `StatusBar`.
    ///
    /// # Arguments
    /// * `name` - The name of the `StatusBar`.
    ///
    /// # Returns
    /// * `Self` - The modified instance of the `StatusBar`.
    pub fn with_name(mut self, name: impl AsRef<str>) -> Self {
        self.name = name.as_ref().to_string();
        self
    }

    /// The first key bound to `action`, in the chat or everywhere.
    fn first_key(&self, action: Action) -> Option<String> {
        let keymap = self.app_context.keymap_config();
        let mut keys = keymap.get_key_of_single_action(ComponentName::Chat, action);
        keys.sort_by_key(ToString::to_string);
        keys.first().map(ToString::to_string)
    }
}

/// Implement the `HandleFocus` trait for the `StatusBar` struct.
/// This trait allows the `StatusBar` to be focused or unfocused.
impl HandleFocus for StatusBar {
    /// Set the `focused` flag for the `StatusBar`.
    fn focus(&mut self) {
        self.focused = true;
    }
    /// Set the `focused` flag for the `StatusBar`.
    fn unfocus(&mut self) {
        self.focused = false;
    }
}

/// Implement the `Component` trait for the `ChatListWindow` struct.
impl Component for StatusBar {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> std::io::Result<()> {
        self.command_tx = Some(tx);
        Ok(())
    }

    fn update(&mut self, action: Action) {
        match action {
            Action::Key(..) => {
                self.status_message = None;
                self.status_message_set_at = None;
            }
            Action::StatusMessage(msg) => {
                self.status_message = Some(msg);
                self.status_message_set_at = Some(Instant::now());
            }
            _ => {}
        }
    }

    /// One line: the latest status message for a few seconds, otherwise
    /// hints for the help and quit keys.
    fn draw(&mut self, frame: &mut ratatui::Frame<'_>, area: Rect) -> std::io::Result<()> {
        if let Some(set_at) = self.status_message_set_at
            && set_at.elapsed() >= std::time::Duration::from_secs(STATUS_MESSAGE_DURATION_SECS)
        {
            self.status_message = None;
            self.status_message_set_at = None;
        }
        let theme = self.app_context.theme();
        let line = match &self.status_message {
            Some(message) => Line::from(Span::styled(
                format!(" {message}"),
                theme.style(HighlightGroup::Info),
            )),
            None => {
                let mut spans = vec![Span::raw(" ")];
                for (index, (action, label)) in [
                    (Action::ShowLeaderMenu, "menu"),
                    (Action::ShowCommandGuide, "help"),
                    (Action::ShowMessageActions, "message actions"),
                    (Action::TryQuit, "quit"),
                ]
                .into_iter()
                .enumerate()
                {
                    let Some(key) = self.first_key(action) else {
                        continue;
                    };
                    if index > 0 {
                        spans.push(Span::styled(" · ", theme.style(HighlightGroup::Decoration)));
                    }
                    spans.push(Span::styled(key, theme.style(HighlightGroup::Shortcut)));
                    spans.push(Span::styled(
                        format!(" {label}"),
                        theme.style(HighlightGroup::Hint),
                    ));
                }
                Line::from(spans)
            }
        };
        frame.render_widget(
            Paragraph::new(line).style(theme.style(HighlightGroup::Normal)),
            area,
        );
        Ok(())
    }
}
