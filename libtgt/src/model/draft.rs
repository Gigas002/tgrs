//! Unsent messages kept per chat (and per forum topic), synced by Telegram
//! across your devices.

use tdlib_rs::{
    enums::DraftMessageContent,
    types::{DraftMessage, DraftMessageContentText, FormattedText},
};

/// The text of a draft; empty for none or a draft that isn't text (e.g. a
/// video note being recorded on a phone).
pub fn draft_text(draft: Option<&DraftMessage>) -> String {
    match draft.map(|draft| &draft.content) {
        Some(DraftMessageContent::Text(text)) => text.text.text.clone(),
        _ => String::new(),
    }
}

/// A draft of `text` written at `date` (Unix time); None, which clears the
/// draft, when there's only whitespace.
pub fn text_draft(text: &str, date: i32) -> Option<DraftMessage> {
    if text.trim().is_empty() {
        return None;
    }
    Some(DraftMessage {
        reply_to: None,
        date,
        content: DraftMessageContent::Text(DraftMessageContentText {
            text: FormattedText {
                text: text.to_owned(),
                entities: Vec::new(),
            },
            link_preview_options: None,
        }),
        effect_id: 0,
        suggested_post_info: None,
    })
}
