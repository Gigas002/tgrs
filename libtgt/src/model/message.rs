use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use chrono::{DateTime, Local};
use tdlib_rs::enums::{MessageContent, MessageReplyTo, MessageSender, TextEntityType};
use tdlib_rs::types::FormattedText;

use super::td_enums::{TdMessageReplyTo, TdMessageSender};

/// Formats a unix timestamp for display: `HH:MM` today, `Yesterday HH:MM`, otherwise the date.
pub fn format_timestamp(timestamp: i32) -> String {
    let d = UNIX_EPOCH + Duration::from_secs(timestamp as u64);
    let datetime = DateTime::<Local>::from(d);
    if datetime.date_naive() == Local::now().date_naive() {
        return datetime.format("%H:%M").to_string();
    }
    if datetime.date_naive() == (Local::now() - chrono::Duration::days(1)).date_naive() {
        return datetime.format("Yesterday %H:%M").to_string();
    }
    datetime.format("%Y-%m-%d %H:%M").to_string()
}

fn path_file_basename(path: &str) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    Path::new(path).file_name()?.to_str().map(String::from)
}

/// Presentation-independent text attributes derived from TDLib text entities.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct TextStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    /// Code, pre-formatted blocks and quotes.
    pub dim: bool,
}

impl TextStyle {
    const BOLD: Self = Self {
        bold: true,
        ..Self::PLAIN
    };
    const ITALIC: Self = Self {
        italic: true,
        ..Self::PLAIN
    };
    const UNDERLINE: Self = Self {
        underline: true,
        ..Self::PLAIN
    };
    const STRIKETHROUGH: Self = Self {
        strikethrough: true,
        ..Self::PLAIN
    };
    const DIM: Self = Self {
        dim: true,
        ..Self::PLAIN
    };
    const PLAIN: Self = Self {
        bold: false,
        italic: false,
        underline: false,
        strikethrough: false,
        dim: false,
    };
}

/// A run of text with one style. Content may contain newlines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSpan {
    pub text: String,
    pub style: TextStyle,
}

impl TextSpan {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: TextStyle::default(),
        }
    }

    pub fn styled(text: impl Into<String>, style: TextStyle) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }
}

/// One logical line of message content.
pub type TextLine = Vec<TextSpan>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageContentType {
    Text,
    Photo {
        file_id: i32,
        file_path: String,
    },
    /// Attached document (general file): TDLib file id, local cache path when downloaded, original file name.
    Document {
        file_id: i32,
        file_path: String,
        file_name: String,
    },
    /// Voice note or voice message (file_id for download, file_path when local, duration in seconds).
    VoiceNote {
        file_id: i32,
        file_path: String,
        duration_secs: i32,
    },
    /// Audio file message (file_id, file_path, duration in seconds).
    Audio {
        file_id: i32,
        file_path: String,
        duration_secs: i32,
    },
    Other,
}

/// A message as tgt keeps it: TDLib data reduced to what the client shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageEntry {
    id: i64,
    sender_id: TdMessageSender,
    content: Vec<TextLine>,
    content_type: MessageContentType,
    reply_to: Option<TdMessageReplyTo>,
    timestamp: i32,
    is_edited: bool,
}

impl MessageEntry {
    /// A plain text message, e.g. for previews and tests.
    pub fn plain(id: i64, sender_id: TdMessageSender, text: &str, timestamp: i32) -> Self {
        Self {
            id,
            sender_id,
            content: vec![vec![TextSpan::plain(text)]],
            content_type: MessageContentType::Text,
            reply_to: None,
            timestamp,
            is_edited: false,
        }
    }

    pub fn id(&self) -> i64 {
        self.id
    }

    pub fn sender(&self) -> TdMessageSender {
        self.sender_id
    }

    pub fn sender_id(&self) -> i64 {
        match self.sender_id {
            TdMessageSender::User(user_id) => user_id,
            TdMessageSender::Chat(chat_id) => chat_id,
        }
    }

    pub fn content(&self) -> &[TextLine] {
        &self.content
    }

    pub fn content_type(&self) -> &MessageContentType {
        &self.content_type
    }

    pub fn reply_to(&self) -> Option<&TdMessageReplyTo> {
        self.reply_to.as_ref()
    }

    /// Unix timestamp of the message.
    pub fn timestamp(&self) -> i32 {
        self.timestamp
    }

    pub fn is_edited(&self) -> bool {
        self.is_edited
    }

    pub fn message_content_to_string(&self) -> String {
        self.content
            .iter()
            .map(|line| line.iter().map(|s| s.text.as_str()).collect::<String>())
            .collect::<Vec<String>>()
            .join("\n")
    }

    /// One-line preview for the pinned strip (handles media where `message_content_to_string` is empty).
    pub fn pinned_strip_preview(&self, max_chars: usize) -> String {
        let text = self.message_content_to_string();
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            let n = trimmed.chars().count();
            if n <= max_chars {
                return trimmed.to_string();
            }
            let take = max_chars.saturating_sub(1);
            return format!("{}…", trimmed.chars().take(take).collect::<String>());
        }
        match self.content_type() {
            MessageContentType::Photo { .. } => "Photo".to_string(),
            MessageContentType::Document { file_name, .. } => {
                if file_name.is_empty() {
                    "Document".to_string()
                } else {
                    file_name.clone()
                }
            }
            MessageContentType::VoiceNote { duration_secs, .. } => {
                format!("Voice message ({duration_secs}s)")
            }
            MessageContentType::Audio { .. } => "Audio file".to_string(),
            MessageContentType::Text => "[Empty]".to_string(),
            MessageContentType::Other => "Attachment".to_string(),
        }
    }

    pub fn set_message_content(&mut self, content: &MessageContent) {
        let (lines, content_type) = message_content_with_type(content);
        self.content = lines;
        self.content_type = content_type;
    }

    pub fn set_is_edited(&mut self, is_edited: bool) {
        self.is_edited = is_edited;
    }

    /// Update the local file path for VoiceNote or Audio content (e.g. after download).
    pub fn set_audio_file_path(&mut self, path: String) {
        match &mut self.content_type {
            MessageContentType::VoiceNote { file_path, .. }
            | MessageContentType::Audio { file_path, .. } => {
                *file_path = path;
            }
            _ => {}
        }
    }

    /// True if this message is a Telegram voice note (OGG Opus). False for audio files (MP3, etc.) and other content.
    pub fn is_voice_note(&self) -> bool {
        matches!(self.content_type(), MessageContentType::VoiceNote { .. })
    }

    /// If this message is voice or audio, returns (file_id, file_path, duration_secs).
    pub fn voice_audio_file_info(&self) -> Option<(i32, String, i32)> {
        match self.content_type() {
            MessageContentType::VoiceNote {
                file_id,
                file_path,
                duration_secs,
            }
            | MessageContentType::Audio {
                file_id,
                file_path,
                duration_secs,
            } => Some((*file_id, file_path.clone(), *duration_secs)),
            _ => None,
        }
    }

    /// Photo or document messages that can be saved to disk: TDLib `file_id` and a default base file name.
    pub fn save_as_candidate(&self) -> Option<(i32, String)> {
        match self.content_type() {
            MessageContentType::Photo { file_id, file_path } => {
                let base = path_file_basename(file_path).unwrap_or_else(|| "photo.jpg".to_string());
                Some((*file_id, base))
            }
            MessageContentType::Document {
                file_id, file_name, ..
            } => {
                let base = if file_name.is_empty() {
                    "document".to_string()
                } else {
                    file_name.clone()
                };
                Some((*file_id, base))
            }
            _ => None,
        }
    }

    /// If the Telegram file is already on disk, returns its local path.
    pub fn local_telegram_file_path(&self) -> Option<String> {
        match self.content_type() {
            MessageContentType::Photo { file_path, .. }
            | MessageContentType::Document { file_path, .. } => {
                if !file_path.is_empty() && Path::new(file_path).is_file() {
                    Some(file_path.clone())
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

impl From<&tdlib_rs::types::Message> for MessageEntry {
    fn from(message: &tdlib_rs::types::Message) -> Self {
        let (content, content_type) = message_content_with_type(&message.content);
        Self {
            id: message.id,
            sender_id: match &message.sender_id {
                MessageSender::User(user) => TdMessageSender::User(user.user_id),
                MessageSender::Chat(chat) => TdMessageSender::Chat(chat.chat_id),
            },
            content,
            content_type,
            reply_to: message.reply_to.as_ref().map(|reply| match reply {
                MessageReplyTo::Message(message) => TdMessageReplyTo::Message(message.into()),
                MessageReplyTo::Story(story) => TdMessageReplyTo::Story(story.into()),
            }),
            timestamp: message.date,
            is_edited: message.edit_date != 0,
        }
    }
}

fn single_line(text: impl Into<String>) -> Vec<TextLine> {
    vec![vec![TextSpan::plain(text)]]
}

fn message_content_with_type(content: &MessageContent) -> (Vec<TextLine>, MessageContentType) {
    match content {
        MessageContent::MessageText(m) => {
            (format_message_content(&m.text), MessageContentType::Text)
        }
        MessageContent::MessagePhoto(photo) => {
            // The last size is usually the largest
            let (file_id, file_path) = photo
                .photo
                .sizes
                .last()
                .map(|size| (size.photo.id, size.photo.local.path.clone()))
                .unwrap_or((0, String::new()));
            (
                single_line("📷 Photo (press Alt+V to view)"),
                MessageContentType::Photo { file_id, file_path },
            )
        }
        MessageContent::MessageAudio(audio) => {
            let duration_secs = audio.audio.duration;
            (
                single_line(format!("🎵 Audio ({duration_secs}s) — Alt+P play/stop")),
                MessageContentType::Audio {
                    file_id: audio.audio.audio.id,
                    file_path: audio.audio.audio.local.path.clone(),
                    duration_secs,
                },
            )
        }
        MessageContent::MessageSticker(_) => (single_line("🎨 Sticker"), MessageContentType::Other),
        MessageContent::MessageVideo(_) => (single_line("🎥 Video"), MessageContentType::Other),
        MessageContent::MessageAnimation(_) => {
            (single_line("🎞️ Animation"), MessageContentType::Other)
        }
        MessageContent::MessageVoiceNote(voice) => {
            let duration_secs = voice.voice_note.duration;
            (
                single_line(format!(
                    "🎤 Voice note ({duration_secs}s) — Alt+P play/stop"
                )),
                MessageContentType::VoiceNote {
                    file_id: voice.voice_note.voice.id,
                    file_path: voice.voice_note.voice.local.path.clone(),
                    duration_secs,
                },
            )
        }
        MessageContent::MessageDocument(doc) => {
            let file_name = doc.document.file_name.clone();
            let label = if file_name.is_empty() {
                "📄 Document".to_string()
            } else {
                format!("📄 {file_name}")
            };
            (
                single_line(label),
                MessageContentType::Document {
                    file_id: doc.document.document.id,
                    file_path: doc.document.document.local.path.clone(),
                    file_name,
                },
            )
        }
        MessageContent::MessageCall(call) => {
            let call_type = if call.is_video {
                "📹 Video Call"
            } else {
                "📞 Call"
            };
            let duration_text = if call.duration > 0 {
                format!(" ({}s)", call.duration)
            } else {
                String::new()
            };
            (
                single_line(format!("{call_type}{duration_text}")),
                MessageContentType::Other,
            )
        }
        MessageContent::MessageVideoChatStarted(_) => (
            single_line("📹 Video chat started"),
            MessageContentType::Other,
        ),
        MessageContent::MessageVideoChatEnded(ended) => {
            let duration_text = if ended.duration > 0 {
                format!(" ({}s)", ended.duration)
            } else {
                String::new()
            };
            (
                single_line(format!("📹 Video chat ended{duration_text}")),
                MessageContentType::Other,
            )
        }
        MessageContent::MessageVideoChatScheduled(scheduled) => (
            single_line(format!(
                "📹 Video chat scheduled for {}",
                format_timestamp(scheduled.start_date)
            )),
            MessageContentType::Other,
        ),
        MessageContent::MessageInviteVideoChatParticipants(_) => (
            single_line("📹 Invited to video chat"),
            MessageContentType::Other,
        ),
        _ => (vec![Vec::new()], MessageContentType::Other),
    }
}

/// Extract a substring by character range (start..end). Used for entity segments.
fn text_slice_chars(text: &str, start: usize, end: usize) -> String {
    text.chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}

/// Style (and replacement text, for links and mentions) of a TDLib text entity.
fn entity_style(entity: &TextEntityType) -> Option<(TextStyle, Option<String>)> {
    let style = match entity {
        TextEntityType::Italic => TextStyle::ITALIC,
        TextEntityType::Bold
        | TextEntityType::Mention
        | TextEntityType::Hashtag
        | TextEntityType::Cashtag => TextStyle::BOLD,
        TextEntityType::Underline
        | TextEntityType::Url
        | TextEntityType::EmailAddress
        | TextEntityType::PhoneNumber
        | TextEntityType::TonAddress
        | TextEntityType::BankCardNumber => TextStyle::UNDERLINE,
        TextEntityType::Strikethrough => TextStyle::STRIKETHROUGH,
        TextEntityType::TextUrl(text_url) => {
            return Some((TextStyle::UNDERLINE, Some(text_url.url.clone())));
        }
        TextEntityType::MentionName(mention_name) => {
            return Some((TextStyle::BOLD, Some(mention_name.user_id.to_string())));
        }
        TextEntityType::Code
        | TextEntityType::Pre
        | TextEntityType::PreCode(_)
        | TextEntityType::BlockQuote
        | TextEntityType::ExpandableBlockQuote => TextStyle::DIM,
        TextEntityType::Spoiler
        | TextEntityType::MediaTimestamp(_)
        | TextEntityType::DateTime(_)
        | TextEntityType::CustomEmoji(_)
        | TextEntityType::BotCommand => return None,
    };
    Some((style, None))
}

fn format_message_content(message: &FormattedText) -> Vec<TextLine> {
    let text = &message.text;
    let entities = &message.entities;

    if entities.is_empty() {
        // Newlines stay inside the span; renderers split them while wrapping
        return if text.is_empty() {
            Vec::new()
        } else {
            single_line(text.clone())
        };
    }

    // Disjoint segments (start, end, style, optional replacement text).
    // TDLib uses UTF-16 offset/length; they are treated as char indices for simplicity.
    type Segment = (usize, usize, TextStyle, Option<String>);
    let mut segments: Vec<Segment> = entities
        .iter()
        .filter_map(|e| {
            let offset = e.offset as usize;
            let end = offset.saturating_add(e.length as usize);
            entity_style(&e.r#type).map(|(style, url)| (offset, end, style, url))
        })
        .collect();

    // Sort by start; merge overlapping (later segment wins for overlap).
    segments.sort_by_key(|(s, _, _, _)| *s);
    let mut disjoint: Vec<Segment> = Vec::new();
    for (start, end, style, url_override) in segments {
        if start >= end {
            continue;
        }
        if let Some(last) = disjoint.last_mut()
            && start < last.1
        {
            // Trim the previous segment so it doesn't overlap with this one
            last.1 = start;
        }
        disjoint.push((start, end, style, url_override));
    }

    // Pre/PreCode blocks (not inline code) get blank lines around them
    let code_blocks: Vec<(usize, usize)> = entities
        .iter()
        .filter(|e| matches!(e.r#type, TextEntityType::Pre | TextEntityType::PreCode(_)))
        .map(|e| {
            let start = e.offset as usize;
            (start, start.saturating_add(e.length as usize))
        })
        .collect();

    // Raw before first, styled(seg), raw between, ... raw after last.
    let char_count = text.chars().count();
    let mut spans: Vec<TextSpan> = Vec::new();
    let mut prev_end = 0usize;

    for (start, end, style, url_override) in disjoint {
        if start > prev_end {
            let raw_slice = text_slice_chars(text, prev_end, start);
            if !raw_slice.is_empty() {
                spans.push(TextSpan::plain(raw_slice));
            }
        }

        let mut content = url_override.unwrap_or_else(|| text_slice_chars(text, start, end));

        if code_blocks.contains(&(start, end)) {
            if !spans.is_empty() {
                content = format!("\n{content}");
            }
            if end < char_count {
                content = format!("{content}\n");
            }
        }

        if !content.is_empty() {
            spans.push(TextSpan::styled(content, style));
        }
        prev_end = end;
    }
    if prev_end < char_count {
        let raw_slice = text_slice_chars(text, prev_end, char_count);
        if !raw_slice.is_empty() {
            spans.push(TextSpan::plain(raw_slice));
        }
    }

    // All spans share one line; newlines within spans are handled by renderers
    vec![spans]
}
