//! Rendering of [`MessageEntry`] values with the active theme.

use chrono::{DateTime, Local, NaiveDate};
use libtgt::model::{
    Media, MediaKind, MessageContentType, MessageEntry, TdMessageReplyTo, TdMessageSender,
    TextLine, TextStyle, format_duration, format_size, format_timestamp,
};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{
    app::context::AppContext,
    settings::{BorderSurface, HighlightGroup, Theme},
};

/// Columns before message content, where the selected message's card border goes.
pub const MESSAGE_PREFIX_WIDTH: u16 = 2;
/// Messages from the same sender closer than this share one header.
pub const AUTHOR_GROUP_MAX_GAP_SECS: i32 = 7 * 60;

pub fn timestamp_span(timestamp: i32, app_context: &AppContext) -> Span<'static> {
    Span::styled(format_timestamp(timestamp), app_context.style_timestamp())
}

/// How message text is drawn: the base style, and the theme's styles for
/// links, mentions and code when they're shown in full (not in previews).
#[derive(Clone, Copy, Default)]
struct ContentStyle {
    base: Style,
    link: Option<Style>,
    mention: Option<Style>,
    code: Option<Style>,
}

impl From<Style> for ContentStyle {
    fn from(base: Style) -> Self {
        Self {
            base,
            ..Self::default()
        }
    }
}

impl ContentStyle {
    fn message_body(theme: &Theme) -> Self {
        Self {
            base: theme.style(HighlightGroup::MessageBody),
            link: Some(theme.style(HighlightGroup::MessageLink)),
            mention: Some(theme.style(HighlightGroup::MentionOther)),
            code: Some(theme.style(HighlightGroup::InlineCode)),
        }
    }

    /// Combines a text style from the message with these styles.
    fn merge(&self, text: TextStyle) -> Style {
        let mut style = self.base;
        let mut modifier = Modifier::empty();
        // Semantic styles from the theme, or a modifier where there's none
        for (enabled, themed, fallback) in [
            (text.link, self.link, Modifier::UNDERLINED),
            (text.mention, self.mention, Modifier::BOLD),
            (text.code, self.code, Modifier::DIM),
        ] {
            if enabled {
                match themed {
                    Some(themed) => style = style.patch(themed),
                    None => modifier.insert(fallback),
                }
            }
        }
        for (enabled, flag) in [
            (text.bold, Modifier::BOLD),
            (text.italic, Modifier::ITALIC),
            (text.underline, Modifier::UNDERLINED),
            (text.strikethrough, Modifier::CROSSED_OUT),
            (text.dim, Modifier::DIM),
        ] {
            if enabled {
                modifier.insert(flag);
            }
        }
        style.add_modifier(modifier)
    }
}

pub fn message_text(
    entry: &MessageEntry,
    myself: bool,
    app_context: &AppContext,
    is_unread: bool, // When myself is false, is_unread is useless
    name_style: Style,
    content_style: Style,
    wrap_width: i32,
) -> Text<'static> {
    let use_emoji = app_context.app_config().use_emoji_icons;
    let (reply_prefix, edited_str, sent_str, seen_str) = if use_emoji {
        ("↩️ Reply to: ", "✏️", "📤", "👀")
    } else {
        ("<-- Reply to: ", "[edited]", "[✓]", "[o o]")
    };
    let (message_reply_name, message_reply_content) = if myself {
        (
            app_context.style_chat_message_myself_reply_name(),
            app_context.style_chat_message_myself_reply_content(),
        )
    } else {
        (
            app_context.style_chat_message_other_reply_name(),
            app_context.style_chat_message_other_reply_content(),
        )
    };

    let reply_text = match entry.reply_to() {
        Some(reply_to) => match reply_to {
            TdMessageReplyTo::Message(message) => {
                if app_context.store().open_chat_id().as_i64() == message.chat_id {
                    let mut reply = Text::default();
                    reply.extend(vec![Line::from(vec![
                        Span::styled(reply_prefix, app_context.style_chat_message_reply_text()),
                        Span::styled(
                            app_context
                                .store()
                                .try_name_from_chats_or_users(
                                    app_context
                                        .store()
                                        .get_message(message.message_id)
                                        .map(|m| m.sender_id())
                                        .unwrap_or(-1),
                                )
                                .unwrap_or_default(),
                            message_reply_name,
                        ),
                    ])]);
                    reply.extend(match app_context.store().get_message(message.message_id) {
                        Some(m) => content_lines(&m, message_reply_content, wrap_width),
                        None => vec![Line::from("")],
                    });
                    Some(reply)
                } else {
                    None
                }
            }
            TdMessageReplyTo::Story(_) => {
                let mut reply = Text::default();
                reply.extend(vec![Line::from(vec![
                    Span::styled(reply_prefix, app_context.style_chat_message_reply_text()),
                    Span::styled("Story", message_reply_name),
                ])]);
                Some(reply)
            }
        },
        None => None,
    };

    let edited_display = if entry.is_edited() { edited_str } else { "" };
    let status_display = match (myself, is_unread) {
        (true, true) => sent_str,
        (true, false) => seen_str,
        (false, _) => "",
    };
    let mut text = Text::default();
    text.extend(vec![Line::from(vec![
        Span::styled(
            match entry.sender() {
                TdMessageSender::User(user_id) => app_context
                    .store()
                    .try_name_from_chats_or_users(user_id)
                    .unwrap_or_default(),
                TdMessageSender::Chat(chat_id) => app_context
                    .store()
                    .name_from_chats(chat_id)
                    .unwrap_or_default(),
            },
            name_style,
        ),
        Span::raw(" "),
        Span::raw(edited_display),
        Span::raw(" "),
        Span::raw(status_display),
        Span::raw(" "),
        timestamp_span(entry.timestamp(), app_context),
    ])]);
    text.extend(reply_text.unwrap_or_default());
    text.extend(content_lines(entry, content_style, wrap_width));
    text
}

/// Message content as styled lines. `wrap_width` -1 only splits on newlines;
/// otherwise lines wrap by display width (so emoji/CJK don't overflow).
pub fn content_lines(
    entry: &MessageEntry,
    content_style: Style,
    wrap_width: i32,
) -> Vec<Line<'static>> {
    wrap_text_lines(entry.content(), &content_style.into(), wrap_width)
}

/// [`content_lines`] for any formatted lines.
fn wrap_text_lines(
    content: &[TextLine],
    content_style: &ContentStyle,
    wrap_width: i32,
) -> Vec<Line<'static>> {
    if wrap_width == -1 {
        // No wrap - split on newlines within spans
        let mut lines = Vec::new();
        for line in content {
            let mut current_line = Line::default();

            for span in line.iter() {
                if span.text.contains('\n') {
                    // Split this span on newlines
                    let parts: Vec<&str> = span.text.split('\n').collect();
                    for (i, part) in parts.iter().enumerate() {
                        if i > 0 {
                            // Push current line and start a new one
                            lines.push(std::mem::take(&mut current_line));
                        }
                        if !part.is_empty() {
                            current_line.spans.push(Span::styled(
                                (*part).to_string(),
                                content_style.merge(span.style),
                            ));
                        }
                    }
                } else {
                    current_line.spans.push(Span::styled(
                        span.text.clone(),
                        content_style.merge(span.style),
                    ));
                }
            }

            if !current_line.spans.is_empty() || lines.is_empty() {
                lines.push(current_line);
            }
        }
        lines
    } else {
        // Wrap by display width (so emoji/CJK don't overflow)
        // Preserve blank lines and newlines within content
        let wrap = wrap_width.max(1) as usize;
        let mut lines = Vec::new();
        let mut current_line = Line::default();
        let mut current_line_width = 0usize;

        for line in content {
            // Preserve blank lines
            if line.is_empty() || line.iter().all(|s| s.text.trim().is_empty()) {
                if !current_line.spans.is_empty() {
                    lines.push(std::mem::take(&mut current_line));
                    current_line_width = 0;
                }
                lines.push(Line::from(""));
                continue;
            }

            // Wrap non-blank lines, handling newlines within spans
            for span in line.iter() {
                for c in span.text.chars() {
                    // Handle newline characters by starting a new line
                    if c == '\n' {
                        lines.push(std::mem::take(&mut current_line));
                        current_line_width = 0;
                        continue;
                    }

                    let w = c.width().unwrap_or(1);
                    if (c == ' ' && current_line_width >= wrap)
                        || (current_line_width + w > wrap && current_line_width > 0)
                    {
                        lines.push(std::mem::take(&mut current_line));
                        current_line_width = 0;
                    }
                    current_line
                        .spans
                        .push(Span::styled(c.to_string(), content_style.merge(span.style)));
                    current_line_width += w;
                }
            }
        }
        if !current_line.spans.is_empty() {
            lines.push(current_line);
        }
        lines
    }
}

/// How a message sits among its neighbors in the chat.
#[derive(Debug, Clone, Copy, Default)]
pub struct MessageRow {
    /// First message of its sender's group: shows the name and time.
    pub show_header: bool,
    /// Last message of its group: followed by a blank line.
    pub bottom_gap: bool,
    /// First message of a day: preceded by a date line.
    pub new_day: bool,
    /// First unread incoming message: preceded by the unread divider.
    pub first_unread: bool,
    pub selected: bool,
    /// The message the composer is replying to.
    pub reply_target: bool,
    /// Rows left blank for the image preview, drawn over them later.
    pub preview_rows: u16,
}

/// The lines of one message, and where its preview rows start.
pub struct MessageLines {
    pub lines: Vec<Line<'static>>,
    /// Index in `lines` of the first preview row.
    pub preview_row: Option<u16>,
}

/// Whether `entry` starts a new sender group after `previous`.
pub fn starts_author_group(previous: Option<&MessageEntry>, entry: &MessageEntry) -> bool {
    previous.is_none_or(|previous| {
        previous.sender_id() != entry.sender_id()
            || entry.timestamp() - previous.timestamp() >= AUTHOR_GROUP_MAX_GAP_SECS
            || local_date(previous.timestamp()) != local_date(entry.timestamp())
    })
}

/// Whether `entry` is on a different day than `previous`.
pub fn starts_new_day(previous: Option<&MessageEntry>, entry: &MessageEntry) -> bool {
    previous
        .is_none_or(|previous| local_date(previous.timestamp()) != local_date(entry.timestamp()))
}

fn local_time(timestamp: i32) -> DateTime<Local> {
    DateTime::from_timestamp(i64::from(timestamp), 0)
        .unwrap_or_default()
        .with_timezone(&Local)
}

fn local_date(timestamp: i32) -> NaiveDate {
    local_time(timestamp).date_naive()
}

/// A message laid out like concord's: an author line, the content indented
/// under it, then the preview rows. The selected message is framed by a card.
/// `width` is the list's inner width.
pub fn message_lines(
    entry: &MessageEntry,
    row: MessageRow,
    app_context: &AppContext,
    width: u16,
) -> MessageLines {
    let theme = app_context.theme();
    let width = usize::from(width).max(8);
    // Room for the card's borders on both sides
    let content_width = width.saturating_sub(2 * usize::from(MESSAGE_PREFIX_WIDTH));
    let store = app_context.store();
    let myself = entry.sender_id() == store.me();

    let mut separators = Vec::new();
    if row.new_day {
        let date = local_time(entry.timestamp())
            .format(" %Y-%m-%d ")
            .to_string();
        separators.push(separator_line(
            &date,
            width,
            theme.style(HighlightGroup::DateDivider),
        ));
    }
    if row.first_unread {
        separators.push(unread_divider_line(width, app_context));
    }

    let mut body: Vec<Vec<Span<'static>>> = Vec::new();
    if row.show_header {
        body.push(header_spans(entry, myself, app_context, content_width));
    }
    if let Some(reply) = reply_spans(entry, app_context, content_width) {
        body.push(reply);
    }
    let caption: &[TextLine] = match entry.content_type() {
        MessageContentType::Media(media) => {
            body.push(vec![Span::styled(
                attachment_label(media),
                theme.style(HighlightGroup::MessageAttachment),
            )]);
            // The first line is the media summary, replaced by the label
            entry.content().get(1..).unwrap_or_default()
        }
        _ => entry.content(),
    };
    body.extend(
        wrap_text_lines(
            caption,
            &ContentStyle::message_body(theme),
            content_width as i32,
        )
        .into_iter()
        .map(|line| line.spans),
    );
    let preview_row = (row.preview_rows > 0).then_some(body.len());
    body.extend(std::iter::repeat_with(Vec::new).take(usize::from(row.preview_rows)));
    body.extend(reaction_lines(entry, theme, content_width));

    let prefix = if row.reply_target {
        Span::styled("▎ ", theme.style(HighlightGroup::ReplyTargetBorder))
    } else {
        Span::raw(" ".repeat(usize::from(MESSAGE_PREFIX_WIDTH)))
    };
    let mut lines: Vec<Line<'static>> = body
        .into_iter()
        .map(|spans| {
            let mut line = vec![prefix.clone()];
            line.extend(spans);
            Line::from(line)
        })
        .collect();
    if row.bottom_gap {
        lines.push(Line::default());
    }

    let mut preview_row = preview_row;
    if row.selected {
        let time = local_time(entry.timestamp()).format("%H:%M").to_string();
        let extra_top = !row.show_header;
        lines = card_lines(
            lines,
            &time,
            width,
            row.bottom_gap,
            row.show_header,
            app_context,
        );
        if extra_top {
            preview_row = preview_row.map(|row| row + 1);
        }
    }
    let offset = separators.len();
    separators.extend(lines);
    MessageLines {
        lines: separators,
        preview_row: preview_row.map(|row| (row + offset) as u16),
    }
}

/// The pin badge and reaction chips like `👍 3`, wrapped to `width`; ours
/// are highlighted.
fn reaction_lines(entry: &MessageEntry, theme: &Theme, width: usize) -> Vec<Vec<Span<'static>>> {
    let mut chips: Vec<Span<'static>> = Vec::new();
    if entry.is_pinned() {
        chips.push(Span::styled("📌", theme.style(HighlightGroup::PinnedBadge)));
    }
    for reaction in entry.reactions() {
        let group = if reaction.chosen {
            HighlightGroup::SelfReaction
        } else {
            HighlightGroup::Reaction
        };
        chips.push(Span::styled(
            format!("{} {}", reaction.emoji, reaction.count),
            theme.style(group),
        ));
    }
    let mut lines: Vec<Vec<Span<'static>>> = Vec::new();
    let mut used = 0;
    for chip in chips {
        let chip_width = chip.width();
        match lines.last_mut() {
            Some(line) if used + 2 + chip_width <= width => {
                line.push(Span::raw("  "));
                line.push(chip);
                used += 2 + chip_width;
            }
            _ => {
                lines.push(vec![chip]);
                used = chip_width;
            }
        }
    }
    lines
}

fn header_spans(
    entry: &MessageEntry,
    myself: bool,
    app_context: &AppContext,
    width: usize,
) -> Vec<Span<'static>> {
    let theme = app_context.theme();
    let store = app_context.store();
    let time = local_time(entry.timestamp()).format("%H:%M").to_string();
    let mut trailer = vec![
        Span::raw(" "),
        Span::styled(time, theme.style(HighlightGroup::MessageTimestamp)),
    ];
    if entry.is_edited() {
        trailer.push(Span::styled(
            " (edited)",
            theme.style(HighlightGroup::Edited),
        ));
    }
    if myself {
        let read = entry.id() <= store.last_read_outbox_message_id();
        let (mark, group) = if read {
            (" ✓✓", HighlightGroup::ReadReceipt)
        } else {
            (" ✓", HighlightGroup::Muted)
        };
        trailer.push(Span::styled(mark, theme.style(group)));
    }
    let trailer_width: usize = trailer.iter().map(Span::width).sum();
    let author_group = if myself {
        HighlightGroup::SelfMessageAuthor
    } else {
        HighlightGroup::MessageAuthor
    };
    let author = truncate_display_width(
        &sender_name(entry, app_context),
        width.saturating_sub(trailer_width).max(1),
    );
    let mut spans = vec![Span::styled(author, theme.style(author_group))];
    spans.extend(trailer);
    spans
}

fn sender_name(entry: &MessageEntry, app_context: &AppContext) -> String {
    let store = app_context.store();
    match entry.sender() {
        TdMessageSender::User(user_id) => store.try_name_from_chats_or_users(user_id),
        TdMessageSender::Chat(chat_id) => store.name_from_chats(chat_id),
    }
    .filter(|name| !name.is_empty())
    .unwrap_or_else(|| "Unknown".to_owned())
}

/// `↪ Name  first line of the replied message`, muted, on one line.
fn reply_spans(
    entry: &MessageEntry,
    app_context: &AppContext,
    width: usize,
) -> Option<Vec<Span<'static>>> {
    let theme = app_context.theme();
    let store = app_context.store();
    let (name, preview) = match entry.reply_to()? {
        TdMessageReplyTo::Message(reply) => {
            if store.open_chat_id().as_i64() != reply.chat_id {
                return None;
            }
            match store.get_message(reply.message_id) {
                Some(original) => (
                    sender_name(&original, app_context),
                    original.pinned_strip_preview(width),
                ),
                None => (String::new(), "Original message".to_owned()),
            }
        }
        TdMessageReplyTo::Story(_) => (String::new(), "Story".to_owned()),
    };
    let secondary = theme.style(HighlightGroup::MessageSecondary);
    let mut spans = vec![Span::styled("↪ ", secondary)];
    let mut used = 2;
    if !name.is_empty() {
        let name = truncate_display_width(&name, width.saturating_sub(used).max(1));
        used += name.width() + 1;
        spans.push(Span::styled(
            name,
            theme.apply(HighlightGroup::Strong, secondary),
        ));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(
        truncate_display_width(&preview, width.saturating_sub(used).max(1)),
        secondary,
    ));
    Some(spans)
}

/// `[photo] 1.2 MiB`, `[file: report.pdf] 2.0 KiB`, `[video] 1:15 · 3.4 MiB`.
pub fn attachment_label(media: &Media) -> String {
    let kind = match media.kind {
        MediaKind::Photo => "photo",
        MediaKind::Video => "video",
        MediaKind::Animation => "gif",
        MediaKind::Audio => "audio",
        MediaKind::Voice => "voice",
        MediaKind::VideoNote => "video message",
        MediaKind::Sticker => "sticker",
        MediaKind::Document => "file",
    };
    let mut label = if media.file_name.is_empty() {
        format!("[{kind}]")
    } else {
        format!("[{kind}: {}]", media.file_name)
    };
    let mut details = Vec::new();
    if media.duration_secs > 0 {
        details.push(format_duration(media.duration_secs));
    }
    if media.file.size > 0 {
        details.push(format_size(media.file.size));
    }
    if !details.is_empty() {
        label.push(' ');
        label.push_str(&details.join(" · "));
    }
    label
}

/// Frames message lines in a card, replacing the selection prefix with the
/// left border and padding to `width` for the right one. A message without a
/// header gets an empty top border and its time on the bottom border.
fn card_lines(
    lines: Vec<Line<'static>>,
    sent_time: &str,
    width: usize,
    has_bottom_gap: bool,
    has_header: bool,
    app_context: &AppContext,
) -> Vec<Line<'static>> {
    let theme = app_context.theme();
    let border = theme.border_set(BorderSurface::Message);
    let border_style = theme.style(HighlightGroup::MessageSelectedBorder);
    let time_style = theme.style(HighlightGroup::MessageTimestamp);
    let last_index = lines.len().saturating_sub(1);
    let border_time = (!has_header).then_some(sent_time);

    let content_line = |line: Line<'static>, top: bool| {
        let mut spans: Vec<Span<'static>> = line.spans.into_iter().skip(1).collect();
        let left = if top {
            format!("{}{}", border.top_left, border.horizontal_top)
        } else {
            format!("{} ", border.vertical_left)
        };
        spans.insert(0, Span::styled(left, border_style));
        let used: usize = spans.iter().map(Span::width).sum();
        let right = if top {
            border.top_right.to_owned()
        } else {
            format!(" {}", border.vertical_right)
        };
        let fill = if top { border.horizontal_top } else { " " };
        let padding = width.saturating_sub(used).saturating_sub(right.width());
        spans.push(Span::styled(fill.repeat(padding), border_style));
        spans.push(Span::styled(right, border_style));
        Line::from(spans)
    };
    let bottom_line = || {
        let inner = width.saturating_sub(2);
        match border_time.filter(|time| inner > time.width() + 3) {
            Some(time) => Line::from(vec![
                Span::styled(
                    format!(
                        "{}{}",
                        border.bottom_left,
                        border
                            .horizontal_bottom
                            .repeat(inner.saturating_sub(time.width() + 3))
                    ),
                    border_style,
                ),
                Span::styled(format!(" {time} "), time_style),
                Span::styled(
                    format!("{}{}", border.horizontal_bottom, border.bottom_right),
                    border_style,
                ),
            ]),
            None => Line::from(Span::styled(
                format!(
                    "{}{}{}",
                    border.bottom_left,
                    border.horizontal_bottom.repeat(inner),
                    border.bottom_right
                ),
                border_style,
            )),
        }
    };

    let mut card = Vec::new();
    if !has_header {
        card.push(Line::from(Span::styled(
            format!(
                "{}{}{}",
                border.top_left,
                border.horizontal_top.repeat(width.saturating_sub(2)),
                border.top_right
            ),
            border_style,
        )));
    }
    for (index, line) in lines.into_iter().enumerate() {
        card.push(if has_bottom_gap && index == last_index {
            bottom_line()
        } else {
            content_line(line, index == 0 && has_header)
        });
    }
    if !has_bottom_gap {
        card.push(bottom_line());
    }
    card
}

/// `──── label ────` across `width`.
fn separator_line(label: &str, width: usize, style: Style) -> Line<'static> {
    let label_width = label.width();
    let dashes = width.max(label_width + 2).saturating_sub(label_width);
    let left = dashes / 2;
    Line::from(Span::styled(
        format!("{}{label}{}", "─".repeat(left), "─".repeat(dashes - left)),
        style,
    ))
}

/// A red rule with a `New` tag on the right, before the first unread message.
fn unread_divider_line(width: usize, app_context: &AppContext) -> Line<'static> {
    const TAG: &str = " New ";
    let theme = app_context.theme();
    let style = theme.style(HighlightGroup::UnreadDivider);
    if width <= TAG.width() + 2 {
        return Line::from(Span::styled("─".repeat(width), style));
    }
    Line::from(vec![
        Span::styled("─".repeat(width - TAG.width()), style),
        Span::styled(TAG, theme.apply(HighlightGroup::Strong, style)),
    ])
}

/// `text` cut to `width` columns, ending in `…` when cut.
pub fn truncate_display_width(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = c.width().unwrap_or(0);
        if used + w + 1 > width {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('…');
    out
}

#[cfg(test)]
#[path = "tests/message.rs"]
mod tests;
