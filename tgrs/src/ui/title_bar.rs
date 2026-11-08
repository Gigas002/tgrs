use {
    crate::{
        app::action::Action,
        app::context::AppContext,
        settings::HighlightGroup,
        ui::component_traits::{Component, HandleFocus},
    },
    ratatui::{
        layout::Rect,
        text::{Line, Span},
        widgets::Paragraph,
    },
    std::{io, sync::Arc},
    tokio::sync::mpsc,
};

/// `TitleBar` is a struct that represents a title bar.
/// It is responsible for managing the layout and rendering of the title bar.
pub struct TitleBar {
    /// The application configuration.
    app_context: Arc<AppContext>,
    /// The name of the `TitleBar`.
    name: String,
    /// An unbounded sender that send action for processing.
    command_tx: Option<mpsc::UnboundedSender<Action>>,
    /// Indicates whether the `TitleBar` is focused or not.
    focused: bool,
}
/// Implementation of `TitleBar` struct.
impl TitleBar {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        let command_tx = None;
        let name = "".to_string();
        let focused = false;

        TitleBar {
            app_context,
            command_tx,
            name,
            focused,
        }
    }
    /// Set the name of the `TitleBar`.
    ///
    /// # Arguments
    /// * `name` - The name of the `TitleBar`.
    ///
    /// # Returns
    /// * `Self` - The modified instance of the `TitleBar`.
    pub fn with_name(mut self, name: impl AsRef<str>) -> Self {
        self.name = name.as_ref().to_string();
        self
    }
}

/// Implement the `HandleFocus` trait for the `TitleBar` struct.
/// This trait allows the `TitleBar` to be focused or unfocused.
impl HandleFocus for TitleBar {
    /// Set the `focused` flag for the `TitleBar`.
    fn focus(&mut self) {
        self.focused = true;
    }
    /// Set the `focused` flag for the `TitleBar`.
    fn unfocus(&mut self) {
        self.focused = false;
    }
}

/// Implement the `Component` trait for the `TitleBar` struct.
impl Component for TitleBar {
    fn register_action_handler(&mut self, tx: mpsc::UnboundedSender<Action>) -> io::Result<()> {
        self.command_tx = Some(tx);
        Ok(())
    }

    /// One line like concord's header: ` tgrs - v1.2.3 ` and who is logged in.
    fn draw(&mut self, frame: &mut ratatui::Frame<'_>, area: Rect) -> io::Result<()> {
        let theme = self.app_context.theme();
        let mut spans = vec![Span::styled(
            format!(" tgrs - v{} ", env!("CARGO_PKG_VERSION")),
            theme.style(HighlightGroup::HeaderTitle),
        )];
        match self.app_context.store().my_name() {
            Some(name) => {
                spans.push(Span::styled(
                    " Connected as ",
                    theme.style(HighlightGroup::HeaderLabel),
                ));
                spans.push(Span::styled(
                    format!("{name} "),
                    theme.style(HighlightGroup::Strong),
                ));
            }
            None => spans.push(Span::styled(
                " Loading... ",
                theme.style(HighlightGroup::HeaderWarning),
            )),
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(theme.style(HighlightGroup::Normal)),
            area,
        );
        Ok(())
    }
}
