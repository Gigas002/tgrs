//! Helpers shared by the component tests in this directory.

use std::sync::Arc;

use libtgt::model::{ChatListEntry, MessageEntry, TdMessageSender};

use crate::app::context::AppContext;

pub fn create_test_app_context() -> Arc<AppContext> {
    AppContext::for_tests()
}

/// A plain text message from user 1.
pub fn create_mock_message(id: i64, content: &str) -> MessageEntry {
    MessageEntry::plain(id, TdMessageSender::User(1), content, 0)
}

pub fn create_mock_chat(chat_id: i64, chat_name: &str) -> ChatListEntry {
    ChatListEntry {
        chat_id,
        chat_name: chat_name.to_owned(),
        ..ChatListEntry::default()
    }
}
