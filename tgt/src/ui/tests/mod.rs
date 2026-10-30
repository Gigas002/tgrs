//! Helpers shared by the component tests in this directory.

use std::sync::Arc;

use libtgt::model::{ChatListEntry, Media, MediaFile, MediaKind, MessageEntry, TdMessageSender};

use crate::app::context::AppContext;

pub fn create_test_app_context() -> Arc<AppContext> {
    AppContext::for_tests()
}

/// A plain text message from user 1.
pub fn create_mock_message(id: i64, content: &str) -> MessageEntry {
    MessageEntry::plain(id, TdMessageSender::User(1), content, 0)
}

/// A document message from user 1.
pub fn create_mock_media_message(id: i64, file_name: &str) -> MessageEntry {
    let media = Media {
        kind: MediaKind::Document,
        file: MediaFile {
            id: 7,
            size: 2048,
            ..MediaFile::default()
        },
        file_name: file_name.to_owned(),
        mime_type: "application/zip".to_owned(),
        duration_secs: 0,
        preview: None,
    };
    MessageEntry::with_media(id, TdMessageSender::User(1), media, 0)
}

pub fn create_mock_chat(chat_id: i64, chat_name: &str) -> ChatListEntry {
    ChatListEntry {
        chat_id,
        chat_name: chat_name.to_owned(),
        ..ChatListEntry::default()
    }
}
