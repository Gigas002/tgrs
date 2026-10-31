use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use chrono::{DateTime, Local};
use tdlib_rs::enums::{
    MessageContent, MessageReplyTo, MessageSender, ReactionType, StickerFormat, TextEntityType,
    ThumbnailFormat,
};
use tdlib_rs::types::{FormattedText, MessageInteractionInfo, Thumbnail};

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
    /// Quotes.
    pub dim: bool,
    /// Inline code and pre-formatted blocks.
    pub code: bool,
    /// URLs, text links, e-mail addresses and phone numbers.
    pub link: bool,
    /// `@username` and mentions of users without one.
    pub mention: bool,
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
    const CODE: Self = Self {
        code: true,
        ..Self::PLAIN
    };
    const LINK: Self = Self {
        link: true,
        ..Self::PLAIN
    };
    const MENTION: Self = Self {
        mention: true,
        ..Self::PLAIN
    };
    const PLAIN: Self = Self {
        bold: false,
        italic: false,
        underline: false,
        strikethrough: false,
        dim: false,
        code: false,
        link: false,
        mention: false,
    };
}

/// A run of text with one style. Content may contain newlines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSpan {
    pub text: String,
    pub style: TextStyle,
    /// Where a link points, when it isn't the text itself (text links).
    pub url: Option<String>,
}

impl TextSpan {
    pub fn plain(text: impl Into<String>) -> Self {
        Self::styled(text, TextStyle::default())
    }

    pub fn styled(text: impl Into<String>, style: TextStyle) -> Self {
        Self {
            text: text.into(),
            style,
            url: None,
        }
    }

    /// The address this span links to, if it's a link.
    pub fn link_target(&self) -> Option<&str> {
        if !self.style.link {
            return None;
        }
        Some(self.url.as_deref().unwrap_or(&self.text))
    }
}

/// One logical line of message content.
pub type TextLine = Vec<TextSpan>;

/// What kind of file a [`Media`] attachment is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaKind {
    Photo,
    Video,
    /// GIF-like animation (TDLib usually stores these as MP4).
    Animation,
    Audio,
    Voice,
    /// Round video message.
    VideoNote,
    Sticker,
    Document,
}

impl MediaKind {
    /// Human-readable name, e.g. for popup titles.
    pub fn label(self) -> &'static str {
        match self {
            Self::Photo => "Photo",
            Self::Video => "Video",
            Self::Animation => "GIF",
            Self::Audio => "Audio",
            Self::Voice => "Voice message",
            Self::VideoNote => "Video message",
            Self::Sticker => "Sticker",
            Self::Document => "File",
        }
    }
}

/// A TDLib file: its id and, once downloaded, where it lives on disk.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MediaFile {
    pub id: i32,
    /// Local path; empty or partial until TDLib finished downloading.
    pub local_path: String,
    pub is_downloaded: bool,
    /// Size in bytes, 0 when unknown.
    pub size: i64,
}

impl MediaFile {
    /// The local path when the file is completely on disk.
    pub fn downloaded_path(&self) -> Option<&str> {
        (self.is_downloaded && !self.local_path.is_empty() && Path::new(&self.local_path).is_file())
            .then_some(self.local_path.as_str())
    }
}

impl From<&tdlib_rs::types::File> for MediaFile {
    fn from(file: &tdlib_rs::types::File) -> Self {
        Self {
            id: file.id,
            local_path: file.local.path.clone(),
            is_downloaded: file.local.is_downloading_completed,
            size: if file.size > 0 {
                file.size
            } else {
                file.expected_size
            },
        }
    }
}

/// The file attached to a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Media {
    pub kind: MediaKind,
    pub file: MediaFile,
    /// Original file name; may be empty (photos, voice messages, ...).
    pub file_name: String,
    pub mime_type: String,
    /// Playback length in seconds; 0 for still media.
    pub duration_secs: i32,
    /// A still image (JPEG, PNG, WebP or GIF) that represents the media, if any.
    /// For photos and static stickers this is the file itself.
    pub preview: Option<MediaFile>,
    /// A smaller version of [`Self::preview`] for showing in the chat, when
    /// Telegram has one (photos come in several sizes).
    pub thumbnail: Option<MediaFile>,
}

impl Media {
    /// The image to show in the chat: the thumbnail, else the preview.
    pub fn inline_preview(&self) -> Option<&MediaFile> {
        self.thumbnail.as_ref().or(self.preview.as_ref())
    }

    /// A file name to save the media under: the original name, else one
    /// derived from the kind, file id and MIME type.
    pub fn suggested_file_name(&self) -> String {
        if !self.file_name.is_empty() {
            return self.file_name.clone();
        }
        if let Some(name) = path_file_basename(&self.file.local_path) {
            return name;
        }
        let stem = match self.kind {
            MediaKind::Photo => "photo",
            MediaKind::Video => "video",
            MediaKind::Animation => "animation",
            MediaKind::Audio => "audio",
            MediaKind::Voice => "voice",
            MediaKind::VideoNote => "video_note",
            MediaKind::Sticker => "sticker",
            MediaKind::Document => "file",
        };
        let extension = match self.kind {
            MediaKind::Photo => Some("jpg"),
            MediaKind::Voice => Some("ogg"),
            MediaKind::VideoNote => Some("mp4"),
            _ => extension_for_mime(&self.mime_type),
        };
        match extension {
            Some(extension) => format!("{stem}_{}.{extension}", self.file.id),
            None => format!("{stem}_{}", self.file.id),
        }
    }

    /// One-line description for the chat, e.g. `🎥 Video (0:12)`.
    pub fn summary(&self) -> String {
        let icon = match self.kind {
            MediaKind::Photo => "📷",
            MediaKind::Video => "🎥",
            MediaKind::Animation => "🎞️",
            MediaKind::Audio => "🎵",
            MediaKind::Voice => "🎤",
            MediaKind::VideoNote => "⏺️",
            MediaKind::Sticker => "🎨",
            MediaKind::Document => "📄",
        };
        let name = match self.kind {
            MediaKind::Audio | MediaKind::Document if !self.file_name.is_empty() => {
                self.file_name.as_str()
            }
            kind => kind.label(),
        };
        let mut summary = format!("{icon} {name}");
        if self.duration_secs > 0 {
            summary.push_str(&format!(" ({})", format_duration(self.duration_secs)));
        } else if self.kind == MediaKind::Document && self.file.size > 0 {
            summary.push_str(&format!(" ({})", format_size(self.file.size)));
        }
        summary
    }
}

/// `m:ss`, or `h:mm:ss` from one hour.
pub fn format_duration(secs: i32) -> String {
    let secs = secs.max(0);
    let (h, m, s) = (secs / 3600, secs % 3600 / 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Byte count with a binary unit, e.g. `1.5 MiB`.
pub fn format_size(bytes: i64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes.max(0) as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn extension_for_mime(mime_type: &str) -> Option<&'static str> {
    Some(match mime_type {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "video/mp4" => "mp4",
        "video/webm" => "webm",
        "video/quicktime" => "mov",
        "audio/mpeg" => "mp3",
        "audio/ogg" => "ogg",
        "audio/mp4" | "audio/x-m4a" => "m4a",
        "audio/flac" | "audio/x-flac" => "flac",
        "application/pdf" => "pdf",
        "application/zip" => "zip",
        _ => return None,
    })
}

/// One reaction under a message, with how many added it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reaction {
    /// The emoji, or [`Reaction::CUSTOM`] / [`Reaction::PAID`] for reactions
    /// that aren't plain emoji.
    pub emoji: String,
    pub count: i32,
    /// Whether the current user added it.
    pub chosen: bool,
}

impl Reaction {
    /// Shown for custom emoji reactions, which only Telegram can draw.
    pub const CUSTOM: &'static str = "✦";
    /// Shown for paid (star) reactions.
    pub const PAID: &'static str = "⭐";

    /// Whether the user can add or remove this reaction by its emoji.
    pub fn is_emoji(&self) -> bool {
        self.emoji != Self::CUSTOM && self.emoji != Self::PAID
    }
}

/// The emoji of a reaction type, as shown and as sent back to TDLib.
pub fn reaction_emoji(reaction: &ReactionType) -> String {
    match reaction {
        ReactionType::Emoji(emoji) => emoji.emoji.clone(),
        ReactionType::CustomEmoji(_) => Reaction::CUSTOM.to_owned(),
        ReactionType::Paid => Reaction::PAID.to_owned(),
    }
}

fn reactions(info: Option<&MessageInteractionInfo>) -> Vec<Reaction> {
    info.and_then(|info| info.reactions.as_ref())
        .map(|reactions| {
            reactions
                .reactions
                .iter()
                .map(|reaction| Reaction {
                    emoji: reaction_emoji(&reaction.r#type),
                    count: reaction.total_count,
                    chosen: reaction.is_chosen,
                })
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageContentType {
    Text,
    Media(Media),
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
    is_pinned: bool,
    reactions: Vec<Reaction>,
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
            is_pinned: false,
            reactions: Vec::new(),
        }
    }

    /// A message with formatted text, e.g. for tests.
    pub fn formatted(
        id: i64,
        sender_id: TdMessageSender,
        content: Vec<TextLine>,
        timestamp: i32,
    ) -> Self {
        Self {
            content,
            ..Self::plain(id, sender_id, "", timestamp)
        }
    }

    /// A media message without caption, e.g. for tests.
    pub fn with_media(id: i64, sender_id: TdMessageSender, media: Media, timestamp: i32) -> Self {
        Self {
            id,
            sender_id,
            content: single_line(media.summary()),
            content_type: MessageContentType::Media(media),
            reply_to: None,
            timestamp,
            is_edited: false,
            is_pinned: false,
            reactions: Vec::new(),
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

    pub fn is_pinned(&self) -> bool {
        self.is_pinned
    }

    pub fn reactions(&self) -> &[Reaction] {
        &self.reactions
    }

    /// Addresses the message links to, in order and without repeats.
    pub fn links(&self) -> Vec<String> {
        let mut links: Vec<String> = Vec::new();
        for target in self
            .content
            .iter()
            .flatten()
            .filter_map(TextSpan::link_target)
        {
            if !links.iter().any(|link| link == target) {
                links.push(target.to_owned());
            }
        }
        links
    }

    /// The attached file, if the message has one.
    pub fn media(&self) -> Option<&Media> {
        match &self.content_type {
            MessageContentType::Media(media) => Some(media),
            _ => None,
        }
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
            MessageContentType::Media(media) => media.summary(),
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

    pub fn set_is_pinned(&mut self, is_pinned: bool) {
        self.is_pinned = is_pinned;
    }

    pub fn set_interaction_info(&mut self, info: Option<&MessageInteractionInfo>) {
        self.reactions = reactions(info);
    }

    /// Sets the reactions directly, e.g. for tests.
    pub fn with_reactions(mut self, reactions: Vec<Reaction>) -> Self {
        self.reactions = reactions;
        self
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
            is_pinned: message.is_pinned,
            reactions: reactions(message.interaction_info.as_ref()),
        }
    }
}

fn single_line(text: impl Into<String>) -> Vec<TextLine> {
    vec![vec![TextSpan::plain(text)]]
}

/// The media summary line followed by the caption.
fn media_content(caption: &FormattedText, media: Media) -> (Vec<TextLine>, MessageContentType) {
    let mut lines = single_line(media.summary());
    lines.extend(format_message_content(caption));
    (lines, MessageContentType::Media(media))
}

/// Thumbnails in formats that decode to a still image.
/// Photo sizes at least this large (in pixels, on the longer side) are sharp
/// enough for the chat preview.
const CHAT_PHOTO_SIZE: i32 = 320;

/// The smallest photo size that still looks sharp in the chat, unless that is
/// the full photo anyway.
fn chat_photo_size(sizes: &[tdlib_rs::types::PhotoSize]) -> Option<MediaFile> {
    let (_, smaller) = sizes.split_last()?;
    smaller
        .iter()
        .filter(|size| size.width.max(size.height) >= CHAT_PHOTO_SIZE)
        .min_by_key(|size| size.width.max(size.height))
        .map(|size| MediaFile::from(&size.photo))
}

pub(crate) fn still_thumbnail(thumbnail: Option<&Thumbnail>) -> Option<MediaFile> {
    thumbnail
        .filter(|thumbnail| {
            matches!(
                thumbnail.format,
                ThumbnailFormat::Jpeg
                    | ThumbnailFormat::Png
                    | ThumbnailFormat::Webp
                    | ThumbnailFormat::Gif
            )
        })
        .map(|thumbnail| MediaFile::from(&thumbnail.file))
}

fn message_content_with_type(content: &MessageContent) -> (Vec<TextLine>, MessageContentType) {
    match content {
        MessageContent::MessageText(m) => {
            (format_message_content(&m.text), MessageContentType::Text)
        }
        MessageContent::MessagePhoto(photo) => {
            // The last size is usually the largest
            let sizes = &photo.photo.sizes;
            let file = sizes
                .last()
                .map(|size| MediaFile::from(&size.photo))
                .unwrap_or_default();
            media_content(
                &photo.caption,
                Media {
                    kind: MediaKind::Photo,
                    preview: Some(file.clone()),
                    thumbnail: chat_photo_size(sizes),
                    file,
                    file_name: String::new(),
                    mime_type: "image/jpeg".into(),
                    duration_secs: 0,
                },
            )
        }
        MessageContent::MessageVideo(video) => media_content(
            &video.caption,
            Media {
                kind: MediaKind::Video,
                file: MediaFile::from(&video.video.video),
                file_name: video.video.file_name.clone(),
                mime_type: video.video.mime_type.clone(),
                duration_secs: video.video.duration,
                preview: still_thumbnail(video.video.thumbnail.as_ref()),
                thumbnail: None,
            },
        ),
        MessageContent::MessageAnimation(animation) => media_content(
            &animation.caption,
            Media {
                kind: MediaKind::Animation,
                file: MediaFile::from(&animation.animation.animation),
                file_name: animation.animation.file_name.clone(),
                mime_type: animation.animation.mime_type.clone(),
                duration_secs: animation.animation.duration,
                preview: still_thumbnail(animation.animation.thumbnail.as_ref()),
                thumbnail: None,
            },
        ),
        MessageContent::MessageAudio(audio) => {
            let audio_info = &audio.audio;
            let file_name = match (audio_info.performer.is_empty(), audio_info.title.is_empty()) {
                (false, false) => format!("{} – {}", audio_info.performer, audio_info.title),
                (true, false) => audio_info.title.clone(),
                _ => audio_info.file_name.clone(),
            };
            media_content(
                &audio.caption,
                Media {
                    kind: MediaKind::Audio,
                    file: MediaFile::from(&audio_info.audio),
                    file_name,
                    mime_type: audio_info.mime_type.clone(),
                    duration_secs: audio_info.duration,
                    preview: still_thumbnail(audio_info.album_cover_thumbnail.as_ref()),
                    thumbnail: None,
                },
            )
        }
        MessageContent::MessageVoiceNote(voice) => media_content(
            &voice.caption,
            Media {
                kind: MediaKind::Voice,
                file: MediaFile::from(&voice.voice_note.voice),
                file_name: String::new(),
                mime_type: voice.voice_note.mime_type.clone(),
                duration_secs: voice.voice_note.duration,
                preview: None,
                thumbnail: None,
            },
        ),
        MessageContent::MessageVideoNote(note) => media_content(
            &FormattedText::default(),
            Media {
                kind: MediaKind::VideoNote,
                file: MediaFile::from(&note.video_note.video),
                file_name: String::new(),
                mime_type: "video/mp4".into(),
                duration_secs: note.video_note.duration,
                preview: still_thumbnail(note.video_note.thumbnail.as_ref()),
                thumbnail: None,
            },
        ),
        MessageContent::MessageSticker(sticker) => {
            let sticker = &sticker.sticker;
            let file = MediaFile::from(&sticker.sticker);
            let (mime_type, preview) = match sticker.format {
                StickerFormat::Webp => ("image/webp", Some(file.clone())),
                StickerFormat::Tgs => (
                    "application/x-tgsticker",
                    still_thumbnail(sticker.thumbnail.as_ref()),
                ),
                StickerFormat::Webm => ("video/webm", still_thumbnail(sticker.thumbnail.as_ref())),
            };
            let media = Media {
                kind: MediaKind::Sticker,
                file,
                file_name: String::new(),
                mime_type: mime_type.into(),
                duration_secs: 0,
                preview,
                thumbnail: None,
            };
            let summary = format!("{} {}", media.summary(), sticker.emoji);
            (
                single_line(summary.trim_end()),
                MessageContentType::Media(media),
            )
        }
        MessageContent::MessageDocument(doc) => media_content(
            &doc.caption,
            Media {
                kind: MediaKind::Document,
                file: MediaFile::from(&doc.document.document),
                file_name: doc.document.file_name.clone(),
                mime_type: doc.document.mime_type.clone(),
                duration_secs: 0,
                preview: still_thumbnail(doc.document.thumbnail.as_ref()),
                thumbnail: None,
            },
        ),
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

/// Style (and link target, for text links) of a TDLib text entity.
fn entity_style(entity: &TextEntityType) -> Option<(TextStyle, Option<String>)> {
    let style = match entity {
        TextEntityType::Italic => TextStyle::ITALIC,
        TextEntityType::Bold | TextEntityType::Hashtag | TextEntityType::Cashtag => TextStyle::BOLD,
        TextEntityType::Mention | TextEntityType::MentionName(_) => TextStyle::MENTION,
        TextEntityType::Url | TextEntityType::EmailAddress | TextEntityType::PhoneNumber => {
            TextStyle::LINK
        }
        TextEntityType::TextUrl(text_url) => {
            return Some((TextStyle::LINK, Some(text_url.url.clone())));
        }
        TextEntityType::Underline | TextEntityType::TonAddress | TextEntityType::BankCardNumber => {
            TextStyle::UNDERLINE
        }
        TextEntityType::Strikethrough => TextStyle::STRIKETHROUGH,
        TextEntityType::Code | TextEntityType::Pre | TextEntityType::PreCode(_) => TextStyle::CODE,
        TextEntityType::BlockQuote | TextEntityType::ExpandableBlockQuote => TextStyle::DIM,
        TextEntityType::Spoiler
        | TextEntityType::MediaTimestamp(_)
        | TextEntityType::DateTime(_)
        | TextEntityType::CustomEmoji(_)
        | TextEntityType::BotCommand => return None,
    };
    Some((style, None))
}

/// Char index of every UTF-16 offset in `text` (TDLib's entity offsets),
/// plus one past the end.
fn utf16_to_char_indices(text: &str) -> Vec<usize> {
    let mut indices = Vec::with_capacity(text.len() + 1);
    for (index, c) in text.chars().enumerate() {
        indices.extend(std::iter::repeat_n(index, c.len_utf16()));
    }
    indices.push(text.chars().count());
    indices
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

    // Disjoint segments (start, end, style, link target), in chars
    type Segment = (usize, usize, TextStyle, Option<String>);
    let chars = utf16_to_char_indices(text);
    let char_range = |offset: i32, length: i32| {
        let to_char = |utf16: usize| chars[utf16.min(chars.len() - 1)];
        let offset = usize::try_from(offset).unwrap_or(0);
        let length = usize::try_from(length).unwrap_or(0);
        (to_char(offset), to_char(offset.saturating_add(length)))
    };
    let mut segments: Vec<Segment> = entities
        .iter()
        .filter_map(|e| {
            let (start, end) = char_range(e.offset, e.length);
            entity_style(&e.r#type).map(|(style, url)| (start, end, style, url))
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
        .map(|e| char_range(e.offset, e.length))
        .collect();

    // Raw before first, styled(seg), raw between, ... raw after last.
    let char_count = text.chars().count();
    let mut spans: Vec<TextSpan> = Vec::new();
    let mut prev_end = 0usize;

    for (start, end, style, url) in disjoint {
        if start > prev_end {
            let raw_slice = text_slice_chars(text, prev_end, start);
            if !raw_slice.is_empty() {
                spans.push(TextSpan::plain(raw_slice));
            }
        }

        let mut content = text_slice_chars(text, start, end);

        if code_blocks.contains(&(start, end)) {
            if !spans.is_empty() {
                content = format!("\n{content}");
            }
            if end < char_count {
                content = format!("{content}\n");
            }
        }

        if !content.is_empty() {
            spans.push(TextSpan {
                url,
                ..TextSpan::styled(content, style)
            });
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
