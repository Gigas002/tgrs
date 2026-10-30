use crate::{
    app::action::Action,
    app::context::AppContext,
    ui::component_name::ComponentName,
    ui::component_traits::{Component, HandleFocus},
};
use libtgt::model::{Media, MessageEntry, format_duration, format_size};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use std::{io, sync::Arc};
use tokio::sync::mpsc::UnboundedSender;

/// Media viewer actions shown in the hint line, in order.
const HINTS: [(Action, &str); 5] = [
    (Action::MediaViewerOpen, "open"),
    (Action::MediaViewerDownload, "download"),
    (Action::MediaViewerPrevious, "previous"),
    (Action::MediaViewerNext, "next"),
    (Action::HideMediaViewer, "close"),
];

/// Popup describing the media of one message, with keys to open it in the
/// default application or download it. With the `media-preview` feature it
/// also shows the photo, or the thumbnail of a video, GIF or sticker.
pub struct MediaViewer {
    app_context: Arc<AppContext>,
    name: String,
    action_tx: Option<UnboundedSender<Action>>,
    focused: bool,
    visible: bool,
    message_id: Option<i64>,
    #[cfg(feature = "media-preview")]
    preview: preview::Preview,
}

impl MediaViewer {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        MediaViewer {
            app_context,
            name: String::new(),
            action_tx: None,
            focused: false,
            visible: false,
            message_id: None,
            #[cfg(feature = "media-preview")]
            preview: preview::Preview::new(),
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

    fn message(&self) -> Option<MessageEntry> {
        self.app_context.store().get_message(self.message_id?)
    }

    fn show(&mut self, message_id: i64) {
        self.visible = true;
        self.message_id = Some(message_id);
        #[cfg(feature = "media-preview")]
        {
            let media = self.message().and_then(|m| m.media().cloned());
            if let Some(action) = self.preview.load(message_id, media.as_ref()) {
                self.send(action);
            }
        }
    }

    fn hide(&mut self) {
        self.visible = false;
        #[cfg(feature = "media-preview")]
        self.preview.clear();
        // Select the last viewed message in the chat
        if let Some(message_id) = self.message_id.take() {
            self.app_context
                .store()
                .set_jump_target_message_id_i64(message_id);
        }
    }

    /// Moves to the closest older (`forward == false`) or newer message with media.
    fn step(&mut self, forward: bool) {
        let Some(current) = self.message_id else {
            return;
        };
        let messages = self.app_context.store().ordered_messages_snapshot();
        let Some(position) = messages.iter().position(|m| m.id() == current) else {
            return;
        };
        let target = if forward {
            messages[position + 1..]
                .iter()
                .find(|m| m.media().is_some())
        } else {
            messages[..position]
                .iter()
                .rev()
                .find(|m| m.media().is_some())
        };
        match target {
            Some(message) => self.show(message.id()),
            None => self.send(Action::StatusMessage(
                if forward {
                    "No newer media in loaded messages."
                } else {
                    "No older media in loaded messages."
                }
                .into(),
            )),
        }
    }

    fn title(media: Option<&Media>) -> String {
        match media {
            Some(media) if media.file_name.is_empty() => format!(" {} ", media.kind.label()),
            Some(media) => format!(" {} · {} ", media.kind.label(), media.file_name),
            None => " Media ".to_owned(),
        }
    }

    fn details(&self, media: &Media) -> Vec<Line<'static>> {
        let label = self.app_context.style_timestamp();
        let field = |name: &str, value: String| {
            Line::from(vec![
                Span::styled(format!("{name}: "), label),
                Span::raw(value),
            ])
        };
        let mut lines = vec![
            field("Type", media.kind.label().to_owned()),
            field("File", media.suggested_file_name()),
        ];
        if media.file.size > 0 {
            lines.push(field("Size", format_size(media.file.size)));
        }
        if media.duration_secs > 0 {
            lines.push(field("Duration", format_duration(media.duration_secs)));
        }
        if !media.mime_type.is_empty() {
            lines.push(field("MIME type", media.mime_type.clone()));
        }
        lines.push(field(
            "Cached",
            media
                .file
                .downloaded_path()
                .map_or_else(|| "not downloaded yet".to_owned(), str::to_owned),
        ));
        lines
    }

    /// `o/enter open · d download · ...` from the `[media_viewer]` keymap.
    fn hint(&self) -> String {
        HINTS
            .iter()
            .filter_map(|(action, label)| {
                let mut keys: Vec<String> = self
                    .app_context
                    .keymap_config()
                    .get_key_of_single_action(ComponentName::MediaViewer, action.clone())
                    .iter()
                    .map(ToString::to_string)
                    .collect();
                keys.sort();
                (!keys.is_empty()).then(|| format!("{} {label}", keys.join("/")))
            })
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

impl HandleFocus for MediaViewer {
    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl Component for MediaViewer {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> io::Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn update(&mut self, action: Action) {
        match action {
            Action::ShowMediaViewer(message_id) => self.show(message_id),
            Action::HideMediaViewer => self.hide(),
            Action::MediaViewerPrevious => self.step(false),
            Action::MediaViewerNext => self.step(true),
            Action::MediaViewerOpen => {
                if let Some(message_id) = self.message_id {
                    self.send(Action::OpenMedia(message_id));
                }
            }
            Action::MediaViewerDownload => {
                if let Some(message_id) = self.message_id {
                    self.send(Action::DownloadMedia(message_id));
                }
            }
            #[cfg(feature = "media-preview")]
            Action::PreviewDecoded(payload) => self.preview.decoded(payload),
            _ => {}
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect) -> io::Result<()> {
        if !self.visible {
            return Ok(());
        }

        let ratio = self.app_context.app_config().media_viewer_popup_size;
        let width = (area.width as f32 * ratio) as u16;
        let height = (area.height as f32 * ratio) as u16;
        let popup_area = Rect::new(
            area.x + area.width.saturating_sub(width) / 2,
            area.y + area.height.saturating_sub(height) / 2,
            width,
            height,
        );
        frame.render_widget(Clear, popup_area);

        let message = self.message();
        let media = message.as_ref().and_then(MessageEntry::media);
        let block = Block::new()
            .borders(Borders::ALL)
            .title(Self::title(media))
            .title_alignment(Alignment::Center)
            .border_style(self.app_context.style_border_component_focused())
            .style(self.app_context.style_chat());
        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        let [body, hint] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .areas(inner);

        match media {
            None => frame.render_widget(
                Paragraph::new("This message has no media.").alignment(Alignment::Center),
                body,
            ),
            #[cfg(feature = "media-preview")]
            Some(media) if self.preview.is_active() => {
                // Preview above a one-line summary
                let [image, summary] = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Min(1), Constraint::Length(1)])
                    .areas(body);
                self.preview
                    .draw(frame, image, self.app_context.style_timestamp());
                frame.render_widget(
                    Paragraph::new(Line::styled(
                        summary_line(media),
                        self.app_context.style_timestamp(),
                    ))
                    .alignment(Alignment::Center),
                    summary,
                );
            }
            Some(media) => frame.render_widget(
                Paragraph::new(self.details(media)).wrap(Wrap { trim: false }),
                body,
            ),
        }

        frame.render_widget(
            Paragraph::new(Line::styled(
                self.hint(),
                self.app_context.style_timestamp(),
            ))
            .alignment(Alignment::Center),
            hint,
        );
        Ok(())
    }
}

/// `name · 1.2 MiB · 0:12`
#[cfg(feature = "media-preview")]
fn summary_line(media: &Media) -> String {
    let mut parts = vec![media.suggested_file_name()];
    if media.file.size > 0 {
        parts.push(format_size(media.file.size));
    }
    if media.duration_secs > 0 {
        parts.push(format_duration(media.duration_secs));
    }
    parts.join(" · ")
}

#[cfg(feature = "media-preview")]
mod preview {
    use libtgt::model::Media;
    use ratatui::{
        Frame,
        layout::{Alignment, Rect},
        style::Style,
        widgets::Paragraph,
    };

    use crate::{
        app::action::{Action, PreviewDecodedPayload, PreviewRequest, PreviewTarget},
        ui::preview::{Image, picker},
    };

    enum State {
        /// Nothing to preview: the viewer shows details instead.
        None,
        Loading {
            message_id: i64,
        },
        Loaded(Image),
        Error(String),
    }

    /// The preview image of the media viewer.
    pub struct Preview {
        state: State,
    }

    impl Preview {
        pub fn new() -> Self {
            // Detect the terminal's image support up front, not on first use
            let _ = picker();
            Self { state: State::None }
        }

        /// Starts loading the preview of `media`; returns the action that
        /// downloads and decodes it.
        pub fn load(&mut self, message_id: i64, media: Option<&Media>) -> Option<Action> {
            let Some(file) = media.and_then(|media| media.preview.as_ref()) else {
                self.state = State::None;
                return None;
            };
            self.state = State::Loading { message_id };
            Some(Action::LoadPreview(PreviewRequest {
                target: PreviewTarget::Viewer,
                message_id,
                file_id: file.id,
                path: file.downloaded_path().map(str::to_owned),
            }))
        }

        pub fn clear(&mut self) {
            self.state = State::None;
        }

        /// Whether a preview is shown (or being loaded) instead of details.
        pub fn is_active(&self) -> bool {
            !matches!(self.state, State::None)
        }

        pub fn decoded(
            &mut self,
            PreviewDecodedPayload { request, result }: PreviewDecodedPayload,
        ) {
            let loading = matches!(
                self.state,
                State::Loading { message_id } if message_id == request.message_id
            );
            if request.target != PreviewTarget::Viewer || !loading {
                return;
            }
            self.state = match result {
                Ok(image) => State::Loaded(Image::new(image)),
                Err(e) => State::Error(e),
            };
        }

        pub fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, text_style: Style) {
            let message = match &mut self.state {
                State::None => return,
                State::Loading { .. } => "Loading preview…".to_owned(),
                State::Error(e) => format!("No preview: {e}"),
                State::Loaded(image) => {
                    image.draw(frame, area, Alignment::Center, false);
                    return;
                }
            };
            let y = area.y + area.height / 2;
            frame.render_widget(
                Paragraph::new(message)
                    .style(text_style)
                    .alignment(Alignment::Center),
                Rect::new(area.x, y, area.width, 1.min(area.height)),
            );
        }
    }
}

#[cfg(test)]
#[path = "tests/media_viewer.rs"]
mod tests;
