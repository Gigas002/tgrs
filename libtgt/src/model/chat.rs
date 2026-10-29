use tdlib_rs::types::User;

use super::message::MessageEntry;

/// One row of the main chat list, built from the store's chat index.
#[derive(Debug, Clone, Default)]
pub struct ChatListEntry {
    pub chat_id: i64,
    pub chat_name: String,
    pub last_message: Option<MessageEntry>,
    /// The other party of a private chat.
    pub user: Option<User>,
    pub is_marked_as_unread: bool,
    pub unread_count: i32,
    /// Whether this chat is pinned in its chat list.
    pub is_pinned: bool,
    /// Identifier of the last read incoming message.
    pub last_read_inbox_message_id: Option<i64>,
    /// Identifier of the last read outgoing message.
    pub last_read_outbox_message_id: Option<i64>,
}
