//! Preview images (`media-preview` feature): the terminal's image protocol,
//! fitting images into cells and the cache of previews shown in the chat.

use std::{
    collections::{HashMap, VecDeque},
    io::IsTerminal,
    sync::OnceLock,
};

use libtgt::model::MediaFile;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    widgets::Paragraph,
};
use ratatui_image::{Resize, StatefulImage, picker::Picker, protocol::StatefulProtocol};

use crate::app::action::{Action, PreviewDecodedPayload, PreviewRequest, PreviewTarget};

/// Rows a preview takes under its message in the chat.
pub const CHAT_PREVIEW_ROWS: u16 = 8;
/// Widest a preview gets in the chat, in columns.
pub const CHAT_PREVIEW_COLUMNS: u16 = 40;
/// Previews kept decoded for the chat; older ones are decoded again when
/// scrolled back to.
const CHAT_CACHE_SIZE: usize = 32;

/// How images are drawn in this terminal, detected once.
pub fn picker() -> &'static Picker {
    static PICKER: OnceLock<Picker> = OnceLock::new();
    PICKER.get_or_init(|| {
        // Only query the terminal when stdout is a TTY: in tests/CI the
        // query can block or time out.
        if std::io::stdout().is_terminal() {
            Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks())
        } else {
            Picker::halfblocks()
        }
    })
}

/// A decoded image, ready to draw.
pub struct Image {
    protocol: Box<StatefulProtocol>,
    width: u32,
    height: u32,
}

impl Image {
    pub fn new(image: image::DynamicImage) -> Self {
        Self {
            width: image.width(),
            height: image.height(),
            protocol: Box::new(picker().new_resize_protocol(image)),
        }
    }

    /// Draws the image as large as fits in `area`, keeping its aspect ratio:
    /// aligned horizontally as `alignment` says, and centered vertically
    /// unless `top` is set.
    pub fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, alignment: Alignment, top: bool) {
        let (width, height) = fit(area, self.width, self.height);
        let x = match alignment {
            Alignment::Left => area.x,
            Alignment::Center => area.x + (area.width - width) / 2,
            Alignment::Right => area.right() - width,
        };
        let y = if top {
            area.y
        } else {
            area.y + (area.height - height) / 2
        };
        frame.render_stateful_widget(
            StatefulImage::new().resize(Resize::Fit(None)),
            Rect::new(x, y, width, height),
            self.protocol.as_mut(),
        );
    }
}

/// The size in cells of an image `width`×`height` pixels scaled to fit `area`.
fn fit(area: Rect, width: u32, height: u32) -> (u16, u16) {
    let font = picker().font_size();
    if width == 0 || height == 0 || font.width == 0 || font.height == 0 {
        return (area.width, area.height);
    }
    let (font_w, font_h) = (f32::from(font.width), f32::from(font.height));
    let scale = (f32::from(area.width) * font_w / width as f32)
        .min(f32::from(area.height) * font_h / height as f32);
    let cells_w = ((width as f32 * scale / font_w).ceil() as u16).min(area.width);
    let cells_h = ((height as f32 * scale / font_h).ceil() as u16).min(area.height);
    (cells_w.max(1), cells_h.max(1))
}

enum Entry {
    Loading,
    Loaded(Image),
    Failed,
}

/// Previews shown in the chat, keyed by file id: message ids repeat across
/// chats, file ids don't.
#[derive(Default)]
pub struct ChatPreviews {
    entries: HashMap<i32, Entry>,
    /// File ids from the least recently requested.
    order: VecDeque<i32>,
}

impl ChatPreviews {
    /// The action that loads the preview of `file`, unless it's loaded or
    /// already loading.
    pub fn request(&mut self, message_id: i64, file: &MediaFile) -> Option<Action> {
        if self.entries.contains_key(&file.id) {
            return None;
        }
        while self.order.len() >= CHAT_CACHE_SIZE {
            if let Some(old) = self.order.pop_front() {
                self.entries.remove(&old);
            }
        }
        self.entries.insert(file.id, Entry::Loading);
        self.order.push_back(file.id);
        Some(Action::LoadPreview(PreviewRequest {
            target: PreviewTarget::Chat,
            message_id,
            file_id: file.id,
            path: file.downloaded_path().map(str::to_owned),
        }))
    }

    pub fn decoded(&mut self, PreviewDecodedPayload { request, result }: PreviewDecodedPayload) {
        if request.target != PreviewTarget::Chat {
            return;
        }
        // Dropped from the cache meanwhile: it's requested again when shown
        if let Some(entry @ Entry::Loading) = self.entries.get_mut(&request.file_id) {
            *entry = match result {
                Ok(image) => Entry::Loaded(Image::new(image)),
                Err(e) => {
                    tracing::debug!("no chat preview for file {}: {e}", request.file_id);
                    Entry::Failed
                }
            };
        }
    }

    /// Draws the preview of `file_id` in `area`, or a placeholder.
    pub fn draw(
        &mut self,
        frame: &mut Frame<'_>,
        file_id: i32,
        area: Rect,
        alignment: Alignment,
        placeholder_style: Style,
    ) {
        let placeholder = match self.entries.get_mut(&file_id) {
            Some(Entry::Loaded(image)) => return image.draw(frame, area, alignment, true),
            Some(Entry::Failed) => "[no preview]",
            Some(Entry::Loading) | None => "[loading preview…]",
        };
        frame.render_widget(
            Paragraph::new(placeholder)
                .style(placeholder_style)
                .alignment(alignment),
            Rect::new(area.x, area.y, area.width, 1.min(area.height)),
        );
    }

    #[cfg(test)]
    pub fn is_loaded(&self, file_id: i32) -> bool {
        matches!(self.entries.get(&file_id), Some(Entry::Loaded(_)))
    }

    #[cfg(test)]
    pub fn cached(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
#[path = "tests/preview.rs"]
mod tests;
