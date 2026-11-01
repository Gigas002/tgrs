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
    /// The text of an unsent message; empty if none.
    pub draft: String,
}

/// What the chat actions see of a chat: its state, which they toggle.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatState {
    pub chat_id: i64,
    pub title: String,
    pub kind: ChatKind,
    /// Notifications are turned off.
    pub is_muted: bool,
    /// The chat is in the Archive chat list.
    pub is_archived: bool,
    /// The chat is pinned in its chat list (the main one or the archive).
    pub is_pinned: bool,
    /// The chat has unread messages or is marked as unread.
    pub is_unread: bool,
    /// The newest message, read when the chat is marked as read; 0 if none.
    pub last_message_id: i64,
    /// The other party of a private or secret chat is on your block list.
    pub is_blocked: bool,
}

impl ChatState {
    /// Whether the chat is with someone who can be blocked: a person or a
    /// bot, not yourself or a group.
    pub fn is_blockable(&self) -> bool {
        matches!(
            self.kind,
            ChatKind::Private | ChatKind::Bot | ChatKind::Secret
        )
    }

    /// Whether leaving is how the chat goes away: groups and channels are
    /// left, other chats are deleted.
    pub fn is_left(&self) -> bool {
        matches!(self.kind, ChatKind::Group | ChatKind::Channel)
    }
}

/// Why a chat can't move among the pinned chats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinMoveError {
    /// The chat isn't pinned; only pinned chats have an order to change.
    NotPinned,
    /// The chat is already the first (moving up) or the last (moving down).
    AtEdge,
}

/// The pinned chats, `pinned` top first, after moving `chat_id` one place up
/// or down.
pub fn move_pinned(pinned: &[i64], chat_id: i64, up: bool) -> Result<Vec<i64>, PinMoveError> {
    let index = pinned
        .iter()
        .position(|id| *id == chat_id)
        .ok_or(PinMoveError::NotPinned)?;
    let other = if up {
        index.checked_sub(1)
    } else {
        Some(index + 1).filter(|other| *other < pinned.len())
    }
    .ok_or(PinMoveError::AtEdge)?;
    let mut moved = pinned.to_vec();
    moved.swap(index, other);
    Ok(moved)
}
