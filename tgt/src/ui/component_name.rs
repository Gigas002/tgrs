use std::fmt::{Display, Formatter, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// `ComponentName` is an enum that represents the name of a component in the
/// user interface.
pub enum ComponentName {
    /// The core window.
    CoreWindow,
    /// The chat list.
    ChatList,
    /// The archived chats, right of the chat list.
    ArchiveList,
    /// The topics of the open forum chat, right of the chat list.
    TopicList,
    /// The chat.
    Chat,
    /// The prompt.
    Prompt,
    /// The reply message window.
    ReplyMessage,
    /// The title bar.
    TitleBar,
    /// The status bar.
    StatusBar,
    /// The command guide popup.
    CommandGuide,
    /// The search overlay (server-side chat message search).
    SearchOverlay,
    /// The actions menu of a chat message.
    MessageActions,
    /// The file upload explorer popup.
    FileUploadExplorer,
    /// Pinned messages browser popup (open chat).
    PinnedMessagesPopup,
    /// Popup to pick a reaction or a link.
    Picker,
    /// Popup to pick a sticker to send.
    StickerPicker,
    /// The actions menu of a chat.
    ChatActions,
    /// The profile of a chat.
    Profile,
    /// Asks for a username to open its chat.
    FindChat,
    /// Edits your own profile.
    EditProfile,
    /// Reports a chat or a message, step by step.
    Report,
    /// The people and chats you've blocked.
    BlockedList,
    /// Your active sessions.
    Sessions,
    /// The leader menu (Space), like concord's.
    LeaderMenu,
}

impl ComponentName {
    /// Every component, in a fixed order.
    pub const ALL: [Self; 24] = [
        Self::CoreWindow,
        Self::ChatList,
        Self::ArchiveList,
        Self::TopicList,
        Self::Chat,
        Self::Prompt,
        Self::ReplyMessage,
        Self::TitleBar,
        Self::StatusBar,
        Self::CommandGuide,
        Self::SearchOverlay,
        Self::MessageActions,
        Self::FileUploadExplorer,
        Self::PinnedMessagesPopup,
        Self::Picker,
        Self::StickerPicker,
        Self::ChatActions,
        Self::Profile,
        Self::FindChat,
        Self::EditProfile,
        Self::Report,
        Self::BlockedList,
        Self::Sessions,
        Self::LeaderMenu,
    ];

    /// The popups (modal overlays); only one is shown at a time.
    pub const POPUPS: [Self; 15] = [
        Self::CommandGuide,
        Self::SearchOverlay,
        Self::MessageActions,
        Self::FileUploadExplorer,
        Self::PinnedMessagesPopup,
        Self::Picker,
        Self::StickerPicker,
        Self::ChatActions,
        Self::Profile,
        Self::FindChat,
        Self::EditProfile,
        Self::Report,
        Self::BlockedList,
        Self::Sessions,
        Self::LeaderMenu,
    ];

    /// Returns true if this component is a popup (overlay) component.
    /// Popup components are modal overlays that should hide other popups when shown.
    pub fn is_popup(&self) -> bool {
        Self::POPUPS.contains(self)
    }
}

impl Display for ComponentName {
    fn fmt(&self, f: &mut Formatter) -> Result {
        match self {
            ComponentName::CoreWindow => write!(f, "Core Window"),
            ComponentName::ChatList => write!(f, "Chat List"),
            ComponentName::ArchiveList => write!(f, "Archive List"),
            ComponentName::TopicList => write!(f, "Topic List"),
            ComponentName::Chat => write!(f, "Chat"),
            ComponentName::Prompt => write!(f, "Prompt"),
            ComponentName::TitleBar => write!(f, "Title Bar"),
            ComponentName::StatusBar => write!(f, "Status Bar"),
            ComponentName::ReplyMessage => write!(f, "Reply Message"),
            ComponentName::CommandGuide => write!(f, "Command Guide"),
            ComponentName::SearchOverlay => write!(f, "Search Overlay"),
            ComponentName::MessageActions => write!(f, "Message Actions"),
            ComponentName::FileUploadExplorer => write!(f, "File Upload Explorer"),
            ComponentName::PinnedMessagesPopup => write!(f, "Pinned Messages"),
            ComponentName::Picker => write!(f, "Picker"),
            ComponentName::StickerPicker => write!(f, "Sticker Picker"),
            ComponentName::ChatActions => write!(f, "Chat Actions"),
            ComponentName::Profile => write!(f, "Profile"),
            ComponentName::FindChat => write!(f, "Find Chat"),
            ComponentName::EditProfile => write!(f, "Edit Profile"),
            ComponentName::Report => write!(f, "Report"),
            ComponentName::BlockedList => write!(f, "Blocked List"),
            ComponentName::Sessions => write!(f, "Sessions"),
            ComponentName::LeaderMenu => write!(f, "Leader Menu"),
        }
    }
}
