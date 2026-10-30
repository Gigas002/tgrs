use std::fmt::{Display, Formatter, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// `ComponentName` is an enum that represents the name of a component in the
/// user interface.
pub enum ComponentName {
    /// The core window.
    CoreWindow,
    /// The chat list.
    ChatList,
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
    /// The leader menu (Space), like concord's.
    LeaderMenu,
}

impl ComponentName {
    /// Every component, in a fixed order.
    pub const ALL: [Self; 14] = [
        Self::CoreWindow,
        Self::ChatList,
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
        Self::LeaderMenu,
    ];

    /// The popups (modal overlays); only one is shown at a time.
    pub const POPUPS: [Self; 7] = [
        Self::CommandGuide,
        Self::SearchOverlay,
        Self::MessageActions,
        Self::FileUploadExplorer,
        Self::PinnedMessagesPopup,
        Self::Picker,
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
            ComponentName::LeaderMenu => write!(f, "Leader Menu"),
        }
    }
}
