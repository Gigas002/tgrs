//! Client-side data model: identifiers, messages and chat list rows.

pub mod chat;
pub mod ids;
pub mod message;
pub mod ordered_chat;
pub mod td_enums;

#[cfg(test)]
mod tests;

pub use chat::ChatListEntry;
pub use ids::{ChatId, MessageId};
pub use message::{
    MessageContentType, MessageEntry, TextLine, TextSpan, TextStyle, format_timestamp,
};
pub use ordered_chat::OrderedChat;
pub use td_enums::{TdChatList, TdMessageReplyTo, TdMessageReplyToMessage, TdMessageSender};
