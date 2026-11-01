//! Client-side data model: identifiers, messages and chat list rows.

pub mod alert;
pub mod chat;
pub mod draft;
pub mod ids;
pub mod message;
pub mod moderation;
pub mod notification_settings;
pub mod ordered_chat;
pub mod profile;
pub mod profile_edit;
pub mod session;
pub mod sticker;
pub mod storage;
pub mod td_enums;
pub mod topic;
pub mod transfer;
pub mod username;

#[cfg(test)]
mod tests;

pub use alert::MessageAlert;
pub use chat::{ChatKind, ChatListEntry, ChatState, PinMoveError, move_pinned};
pub use draft::{draft_text, text_draft};
pub use ids::{ChatId, MessageId};
pub use message::{
    Media, MediaFile, MediaKind, MessageContentType, MessageEntry, Reaction, TextLine, TextSpan,
    TextStyle, format_duration, format_size, format_timestamp, reaction_emoji,
};
pub use moderation::{BlockedSender, ReportOption, ReportStep};
pub use notification_settings::{
    ChatSettings, NotificationOption, NotificationScope, NotificationSettings, NotificationTarget,
    ScopeSettings, apply_chat_option, apply_scope_option,
};
pub use ordered_chat::OrderedChat;
pub use profile::{Profile, ProfileSource};
pub use profile_edit::{ProfileChange, ProfileEdit, ProfileField, profile_change_failure};
pub use session::{DeviceKind, SessionEntry, SignInAlert, session_entries};
pub use sticker::{StickerEntry, StickerSetEntry};
pub use storage::{ChatStorage, StorageUsage};
pub use td_enums::{TdChatList, TdMessageReplyTo, TdMessageReplyToMessage, TdMessageSender};
pub use topic::TopicEntry;
pub use transfer::{Transfer, TransferPurpose, TransferState};
pub use username::parse_username;
