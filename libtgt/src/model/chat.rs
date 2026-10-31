use tdlib_rs::types::User;

use super::message::MessageEntry;

/// What a chat is, for its icon.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum ChatKind {
    /// A private chat with a person.
    #[default]
    Private,
    /// A private chat with a bot.
    Bot,
    /// The chat with yourself.
    Saved,
    /// An end-to-end encrypted private chat.
    Secret,
    /// A basic group or a supergroup.
    Group,
    /// A channel.
    Channel,
    /// Not a chat: the row standing for the Archive chat list.
    Archive,
}

impl ChatKind {
    /// The icon shown before the chat's name.
    pub fn icon(self) -> &'static str {
        match self {
            Self::Private => "👤",
            Self::Bot => "🤖",
            Self::Saved => "🔖",
            Self::Secret => "🔒",
            Self::Group => "👥",
            Self::Channel => "📢",
            Self::Archive => "📦",
        }
    }

    /// Whether messages in the chat can come from several people.
    pub fn is_group(self) -> bool {
        self == Self::Group
    }
}

/// One row of a chat list, built from the store's chat indexes.
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
    pub kind: ChatKind,
    /// Notifications are turned off.
    pub is_muted: bool,
    /// Unread messages that mention you.
    pub unread_mention_count: i32,
    /// Shown instead of the last message (the archive row lists its chats).
    pub summary: Option<String>,
}
