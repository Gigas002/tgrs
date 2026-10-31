//! Client-side data model: identifiers, messages and chat list rows.

pub mod alert;
pub mod chat;
pub mod ids;
pub mod message;
pub mod ordered_chat;
pub mod sticker;
pub mod td_enums;
pub mod topic;

#[cfg(test)]
mod tests;

pub use alert::MessageAlert;
pub use chat::{ChatKind, ChatListEntry};
pub use ids::{ChatId, MessageId};
pub use message::{
    Media, MediaFile, MediaKind, MessageContentType, MessageEntry, Reaction, TextLine, TextSpan,
    TextStyle, format_duration, format_size, format_timestamp, reaction_emoji,
};
pub use ordered_chat::OrderedChat;
pub use sticker::{StickerEntry, StickerSetEntry};
pub use td_enums::{TdChatList, TdMessageReplyTo, TdMessageReplyToMessage, TdMessageSender};
pub use topic::TopicEntry;
