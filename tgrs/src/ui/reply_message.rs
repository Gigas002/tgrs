use {
    crate::{
        app::action::Action,
        app::context::AppContext,
        settings::HighlightGroup,
        ui::component_traits::{Component, HandleFocus},
        ui::pane,
    },
    ratatui::{
        layout::Rect,
        text::{Line, Span, Text},
        widgets::{Paragraph, Wrap},
    },
    std::{io, sync::Arc},
    tokio::sync::mpsc,
};

/// `ReplyMessage` is a struct that represents a window for replying to messages.
/// It is responsible for managing the layout and rendering of the reply message window.
pub struct ReplyMessage {
    /// The application configuration.
    app_context: Arc<AppContext>,
    /// The name of the `ReplyMessage`.
    name: String,
    /// An unbounded sender that send action for processing.
    command_tx: Option<mpsc::UnboundedSender<Action>>,
    /// Indicates whether the `ReplyMessage` is focused or not.
    focused: bool,
}
/// Implementation of `ReplyMessage` struct.
impl ReplyMessage {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        let command_tx = None;
        let name = "".to_string();
        let focused = false;
        ReplyMessage {
            app_context,
            command_tx,
            name,
            focused,
        }
    }
    /// Set the name of the `ReplyMessage`.
    ///
    /// # Arguments
    /// * `name` - The name of the `ReplyMessage`.
    ///
    /// # Returns
    /// * `Self` - The modified instance of the `ReplyMessage`.
    pub fn with_name(mut self, name: impl AsRef<str>) -> Self {
        self.name = name.as_ref().to_string();
        self
    }
}

/// Implement the `HandleFocus` trait for the `ReplyMessage` struct.
/// This trait allows the `ReplyMessage` to be focused or unfocused.
impl HandleFocus for ReplyMessage {
    /// Set the `focused` flag for the `ReplyMessage`.
    fn focus(&mut self) {
        self.focused = true;
    }
    /// Set the `focused` flag for the `ReplyMessage`.
    fn unfocus(&mut self) {
        self.focused = false;
    }
}

/// Implement the `Component` trait for the `ReplyMessage` struct.
impl Component for ReplyMessage {
    fn register_action_handler(&mut self, tx: mpsc::UnboundedSender<Action>) -> io::Result<()> {
        self.command_tx = Some(tx);
        Ok(())
    }

    fn draw(&mut self, frame: &mut ratatui::Frame<'_>, area: Rect) -> io::Result<()> {
        let theme = self.app_context.theme();
        // `↪ text` with the reply-target bar's color, like the message it marks
        let text = Text::from(Line::from(vec![
            Span::styled("↪ ", theme.style(HighlightGroup::ReplyTargetBorder)),
            Span::styled(
                (*self.app_context.store().reply_message_text()).to_string(),
                theme.style(HighlightGroup::ChatPreview),
            ),
        ]));
        let block = pane::block(theme, pane::title("Replying to"), false);
        let paragraph = Paragraph::new(text).block(block).wrap(Wrap { trim: true });

        frame.render_widget(paragraph, area);

        Ok(())
    }
}
