//! Rendering of [`MessageEntry`] values with the active theme.

use libtgt::model::{MessageEntry, TdMessageReplyTo, TdMessageSender, TextStyle, format_timestamp};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use unicode_width::UnicodeWidthChar;

use crate::app::context::AppContext;

pub fn timestamp_span(timestamp: i32, app_context: &AppContext) -> Span<'static> {
    Span::styled(format_timestamp(timestamp), app_context.style_timestamp())
}

/// Combines a text style from the message with the theme's content style.
fn merge_style(text: TextStyle, content: Style) -> Style {
    let mut modifier = Modifier::empty();
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
    Style {
        add_modifier: content.add_modifier | modifier,
        ..content
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
    if wrap_width == -1 {
        // No wrap - split on newlines within spans
        let mut lines = Vec::new();
        for line in entry.content() {
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
                                merge_style(span.style, content_style),
                            ));
                        }
                    }
                } else {
                    current_line.spans.push(Span::styled(
                        span.text.clone(),
                        merge_style(span.style, content_style),
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

        for line in entry.content() {
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
                    current_line.spans.push(Span::styled(
                        c.to_string(),
                        merge_style(span.style, content_style),
                    ));
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
