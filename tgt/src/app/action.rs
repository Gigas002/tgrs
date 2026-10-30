use std::str::FromStr;

use crossterm::event::{KeyCode, KeyModifiers};
use libtgt::model::{TdChatList, TdMessageReplyToMessage};
use ratatui::layout::Rect;

use crate::{app::error::AppError, ui::component_name::ComponentName};

/// Something to do with one chat message, from a chat key or the message actions menu.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum MessageCommand {
    Reply,
    Edit,
    Copy,
    DeleteForEveryone,
    DeleteForMe,
    /// Open the attached media with the default application.
    OpenMedia,
    /// Show the attached media in the media viewer.
    ViewMedia,
    /// Save the attached media to the download directory.
    DownloadMedia,
}

impl MessageCommand {
    /// Every command, in the order of the message actions menu.
    pub const ALL: [Self; 8] = [
        Self::Reply,
        Self::Edit,
        Self::Copy,
        Self::OpenMedia,
        Self::ViewMedia,
        Self::DownloadMedia,
        Self::DeleteForEveryone,
        Self::DeleteForMe,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Reply => "reply",
            Self::Edit => "edit message",
            Self::Copy => "copy message",
            Self::DeleteForEveryone => "delete for everyone",
            Self::DeleteForMe => "delete for me",
            Self::OpenMedia => "open media",
            Self::ViewMedia => "view media",
            Self::DownloadMedia => "download file",
        }
    }
}

/// Where a preview image is shown, which also sets how large it is decoded.
#[cfg(feature = "media-preview")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PreviewTarget {
    /// The media viewer popup.
    Viewer,
    /// Inline in the chat, under the message.
    Chat,
}

/// A preview image to download (when `path` is not on disk) and decode.
#[cfg(feature = "media-preview")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewRequest {
    pub target: PreviewTarget,
    pub message_id: i64,
    pub file_id: i32,
    pub path: Option<String>,
}

/// Payload for [`Action::PreviewDecoded`]: the request and its decode result.
/// [`Eq`] compares the request and error message only (Ok images are not compared).
#[cfg(feature = "media-preview")]
#[derive(Debug, Clone)]
pub struct PreviewDecodedPayload {
    pub request: PreviewRequest,
    pub result: Result<image::DynamicImage, String>,
}

#[cfg(feature = "media-preview")]
impl PartialEq for PreviewDecodedPayload {
    fn eq(&self, other: &Self) -> bool {
        self.request == other.request
            && match (&self.result, &other.result) {
                (Ok(_), Ok(_)) => true,
                (Err(a), Err(b)) => a == b,
                _ => false,
            }
    }
}

#[cfg(feature = "media-preview")]
impl Eq for PreviewDecodedPayload {}

#[derive(Debug, Clone, Eq, PartialEq)]
/// `Modifiers` is a struct that represents the modifiers of a key event.
/// It is used to determine the state of the modifiers when a key event is
/// generated.
pub struct Modifiers {
    /// A boolean that represents the shift modifier.
    pub shift: bool,
    /// A boolean that represents the control modifier.
    pub control: bool,
    /// A boolean that represents the alt modifier.
    pub alt: bool,
    /// A boolean that represents the super modifier.
    pub super_: bool,
    /// A boolean that represents the hyper modifier.
    pub hyper: bool,
    /// A boolean that represents the meta modifier.
    pub meta: bool,
}
/// Implement the `From` trait for `KeyModifiers`
impl From<KeyModifiers> for Modifiers {
    fn from(modifiers: KeyModifiers) -> Self {
        Modifiers {
            shift: modifiers.contains(KeyModifiers::SHIFT),
            control: modifiers.contains(KeyModifiers::CONTROL),
            alt: modifiers.contains(KeyModifiers::ALT),
            super_: modifiers.contains(KeyModifiers::SUPER),
            hyper: modifiers.contains(KeyModifiers::HYPER),
            meta: modifiers.contains(KeyModifiers::META),
        }
    }
}
/// Implement the `From` trait for `Modifiers`
impl From<Modifiers> for KeyModifiers {
    fn from(modifiers: Modifiers) -> Self {
        let mut key_modifiers = KeyModifiers::empty();
        if modifiers.shift {
            key_modifiers.insert(KeyModifiers::SHIFT);
        }
        if modifiers.control {
            key_modifiers.insert(KeyModifiers::CONTROL);
        }
        if modifiers.alt {
            key_modifiers.insert(KeyModifiers::ALT);
        }
        if modifiers.super_ {
            key_modifiers.insert(KeyModifiers::SUPER);
        }
        if modifiers.hyper {
            key_modifiers.insert(KeyModifiers::HYPER);
        }
        if modifiers.meta {
            key_modifiers.insert(KeyModifiers::META);
        }
        key_modifiers
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
// Action` is an enum that represents an action that can be handled by the
/// main application loop and the components of the user interface.
pub enum Action {
    /// Unknown action.
    Unknown,
    /// Init action.
    Init,
    /// Quit action.
    Quit,
    /// TryQuit action, it is used to try to quit the application.
    /// It asks the the core window to confirm the quit action.
    /// If the prompt is not focused, we can quit.
    TryQuit,
    /// Render action.
    Render,
    /// Refresh action: requests a TUI redraw (e.g. from a timer so status bar updates).
    Refresh,
    /// Resize action with width and height.
    Resize(u16, u16),
    /// Paste action with a `String`.
    Paste(String),
    /// Focus Lost action.
    FocusLost,
    /// Focus Gained action.
    FocusGained,

    /// GetMe action.
    GetMe,
    /// LoadChats action with a `ChatList` and a limit.
    LoadChats(TdChatList, i32),
    /// SendMessage action with a `String`.
    /// The first parameter is the `text`.
    /// The second parameter is the `reply_to` field.
    SendMessage(String, Option<TdMessageReplyToMessage>),
    /// SendMessageEdited action with a `i64` and a `String`.
    /// The first parameter is the `message_id` and the second parameter is the `text`.
    SendMessageEdited(i64, String),
    /// GetChatHistory action (load older messages).
    GetChatHistory,
    /// GetChatHistoryNewer action (load newer messages when scrolling near bottom).
    GetChatHistoryNewer,
    /// Chat history was appended (from background task); triggers one render.
    ChatHistoryAppended,
    /// DeleteMessages action.
    /// The first parameter is the `message_ids` and the second parameter is the `revoke`.
    /// If `revoke` is true, the message will be deleted for everyone.
    /// If `revoke` is false, the message will be deleted only for the current user.
    DeleteMessages(Vec<i64>, bool),
    /// ViewAllMessages action.
    ViewAllMessages,

    /// Focus action with a `ComponentName`.
    FocusComponent(ComponentName),
    /// Unfocus action.
    UnfocusComponent,
    /// Toggle ChatList action.
    ToggleChatList,
    /// Increase ChatList size action.
    IncreaseChatListSize,
    /// Decrease ChatList size action.
    DecreaseChatListSize,
    /// Increase Prompt size action.
    IncreasePromptSize,
    /// Decrease Prompt size action.
    DecreasePromptSize,
    /// Key action with a key code.
    Key(KeyCode, Modifiers),
    /// Update area action with a rectangular area.
    UpdateArea(Rect),
    /// HideChatWindowReply action.
    HideChatWindowReply,
    /// ShowCommandGuide action.
    ShowCommandGuide,
    /// HideCommandGuide action.
    HideCommandGuide,

    /// ChatListNext action.
    ChatListNext,
    /// ChatListPrevious action.
    ChatListPrevious,
    /// ChatListSelect action.
    ChatListUnselect,
    /// ChatListOpen action.
    ChatListOpen,
    /// ChatListSortWithString action.
    ChatListSortWithString(String),

    /// ChatWindowNext action.
    ChatWindowNext,
    /// ChatWindowPrevious action.
    ChatWindowPrevious,
    /// ChatWindowUnselect action.
    ChatWindowUnselect,
    /// Run a command on the message selected in the chat window.
    SelectedMessage(MessageCommand),
    /// Run a command on a message, e.g. picked in the message actions menu.
    RunMessageCommand(MessageCommand, i64),

    /// EditMessage action with a `String`.
    /// This action is used to edit a message.
    /// The first parameter is the `message_id` and the second parameter is the `text`.
    EditMessage(i64, String),
    /// ReplyMessage event with a `String`.
    /// This event is used to reply to a message.
    /// The first parameter is the `message_id` and the second parameter is the `text`.
    ReplyMessage(i64, String),
    /// ChatListSearch event.
    /// This event is used to set the prompt to search to set the search string
    /// for the ChatListWindow.
    ChatListSearch,
    /// ChatListRestoreSort event.
    /// This event is used to restore the default ordering.
    /// I.e. pinned first then chronological order.
    ChatListRestoreSort,
    /// ChatWindowSearch event.
    /// This event is used to set the prompt to search mode for filtering messages
    /// in the ChatWindow.
    ChatWindowSearch,
    /// ChatWindowSortWithString action.
    /// This action is used to filter messages in the chat window based on a search string.
    ChatWindowSortWithString(String),
    /// ChatWindowRestoreSort event.
    /// This event is used to restore the default message ordering in the chat window.
    ChatWindowRestoreSort,

    /// Search chat messages via TDLib (server-side). Query is sent to backend; results via SearchResults.
    SearchChatMessages(String),
    /// Search results from TDLib (message IDs + entries for display in search overlay).
    SearchResults(Vec<libtgt::model::MessageEntry>),
    /// Jump to a message: load a window around it and select it (for unloaded messages).
    JumpToMessage(i64),
    /// Jump completed: ChatWindow should select this message_id.
    JumpCompleted(i64),
    /// Close the search overlay (Esc or after jump).
    CloseSearchOverlay,
    /// Show the search overlay (open server search UI).
    ShowSearchOverlay,
    /// Submit search query (Enter when no results).
    SearchOverlaySubmit,
    /// Confirm selection and jump to message (Enter when results exist).
    SearchOverlayConfirm,

    /// Open the actions menu for the message selected in the chat window.
    ShowMessageActions,
    /// Open the actions menu for this message.
    ShowMessageActionsFor(i64),
    /// Close the message actions menu.
    HideMessageActions,
    /// Run the action highlighted in the message actions menu.
    MessageActionsConfirm,
    /// Highlight the previous action in the message actions menu.
    MessageActionsPrevious,
    /// Highlight the next action in the message actions menu.
    MessageActionsNext,

    /// Open the media of a message with the default application (downloads it first).
    OpenMedia(i64),
    /// Save the media of a message to the download directory (downloads it first).
    DownloadMedia(i64),
    /// Show the media viewer for a message.
    ShowMediaViewer(i64),
    /// Close the media viewer.
    HideMediaViewer,
    /// Show the previous (older) message with media in the media viewer.
    MediaViewerPrevious,
    /// Show the next (newer) message with media in the media viewer.
    MediaViewerNext,
    /// Open the media shown in the media viewer with the default application.
    MediaViewerOpen,
    /// Save the media shown in the media viewer to the download directory.
    MediaViewerDownload,
    /// Download and decode a preview image off the main thread.
    #[cfg(feature = "media-preview")]
    LoadPreview(PreviewRequest),
    /// A preview image was decoded in the background.
    #[cfg(feature = "media-preview")]
    PreviewDecoded(PreviewDecodedPayload),

    /// Load pinned messages for the open chat (TDLib search with pinned filter).
    LoadPinnedMessages,
    /// Open the pinned messages popup (Shift+Tab in chat or chat list when pins exist).
    ShowPinnedMessagesPopup,
    /// Close the pinned messages popup.
    HidePinnedMessagesPopup,
    /// Previous pinned message in the popup (older pin when sorted newest-first).
    PinnedPopupPrevious,
    /// Next pinned message in the popup (newer pin when sorted newest-first).
    PinnedPopupNext,
    /// Jump to the currently shown pinned message in the chat (then close popup).
    PinnedPopupConfirmJump,
    /// Show the media of the current pinned message in the media viewer.
    PinnedPopupViewMedia,
    /// Open the media of the current pinned message with the default application.
    PinnedPopupOpenMedia,

    /// StatusMessage: short message to show in the status bar (e.g. "Message yanked").
    StatusMessage(String),
    /// Send the prompt contents using the current prompt mode.
    PromptSendMessage,
    /// PromptCopy: copy selected text in the prompt (overrides try_quit when prompt focused).
    PromptCopy,

    /// Show the file upload explorer popup.
    ShowFileUploadExplorer,
    /// Hide the file upload explorer popup.
    HideFileUploadExplorer,

    /// UploadFile action with a selected local path.
    UploadFile(String),
}
/// Implement the `Action` enum.
impl Action {
    /// Create an action from a key event.
    ///
    /// # Arguments
    /// * `key` - A `KeyCode` that represents the key code.
    /// * `modifiers` - A `KeyModifiers` struct that represents the modifiers.
    ///
    /// # Returns
    /// * `Action` - An action.
    pub fn from_key_event(key: KeyCode, modifiers: KeyModifiers) -> Self {
        Action::Key(key, Modifiers::from(modifiers))
    }
}

/// Implement the `FromStr` trait for `Action`.
impl FromStr for Action {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "quit" => Ok(Action::Quit),
            "try_quit" => Ok(Action::TryQuit),
            "render" => Ok(Action::Render),
            "focus_chat_list" => Ok(Action::FocusComponent(ComponentName::ChatList)),
            "focus_chat" => Ok(Action::FocusComponent(ComponentName::Chat)),
            "focus_prompt" => Ok(Action::FocusComponent(ComponentName::Prompt)),
            "unfocus_component" => Ok(Action::UnfocusComponent),
            "toggle_chat_list" => Ok(Action::ToggleChatList),
            "increase_chat_list_size" => Ok(Action::IncreaseChatListSize),
            "decrease_chat_list_size" => Ok(Action::DecreaseChatListSize),
            "increase_prompt_size" => Ok(Action::IncreasePromptSize),
            "decrease_prompt_size" => Ok(Action::DecreasePromptSize),
            "chat_list_next" => Ok(Action::ChatListNext),
            "chat_list_previous" => Ok(Action::ChatListPrevious),
            "chat_list_unselect" => Ok(Action::ChatListUnselect),
            "chat_list_open" => Ok(Action::ChatListOpen),
            "chat_list_search" => Ok(Action::ChatListSearch),
            "chat_list_restore_sort" => Ok(Action::ChatListRestoreSort),
            "chat_window_search" => Ok(Action::ChatWindowSearch),
            "chat_window_restore_sort" => Ok(Action::ChatWindowRestoreSort),
            "chat_window_next" => Ok(Action::ChatWindowNext),
            "chat_window_previous" => Ok(Action::ChatWindowPrevious),
            "chat_window_unselect" => Ok(Action::ChatWindowUnselect),
            "chat_window_delete_for_everyone" => {
                Ok(Action::SelectedMessage(MessageCommand::DeleteForEveryone))
            }
            "chat_window_delete_for_me" => Ok(Action::SelectedMessage(MessageCommand::DeleteForMe)),
            "chat_window_copy" => Ok(Action::SelectedMessage(MessageCommand::Copy)),
            "chat_window_edit" => Ok(Action::SelectedMessage(MessageCommand::Edit)),
            "chat_window_reply" => Ok(Action::SelectedMessage(MessageCommand::Reply)),
            "open_message_media" => Ok(Action::SelectedMessage(MessageCommand::OpenMedia)),
            "view_message_media" => Ok(Action::SelectedMessage(MessageCommand::ViewMedia)),
            "download_message_media" => Ok(Action::SelectedMessage(MessageCommand::DownloadMedia)),
            "show_message_actions" => Ok(Action::ShowMessageActions),
            "hide_message_actions" => Ok(Action::HideMessageActions),
            "message_actions_confirm" => Ok(Action::MessageActionsConfirm),
            "message_actions_previous" => Ok(Action::MessageActionsPrevious),
            "message_actions_next" => Ok(Action::MessageActionsNext),
            "hide_media_viewer" => Ok(Action::HideMediaViewer),
            "media_viewer_previous" => Ok(Action::MediaViewerPrevious),
            "media_viewer_next" => Ok(Action::MediaViewerNext),
            "media_viewer_open" => Ok(Action::MediaViewerOpen),
            "media_viewer_download" => Ok(Action::MediaViewerDownload),
            "show_command_guide" => Ok(Action::ShowCommandGuide),
            "hide_command_guide" => Ok(Action::HideCommandGuide),
            "prompt_send_message" => Ok(Action::PromptSendMessage),
            "prompt_copy" => Ok(Action::PromptCopy),
            "close_search_overlay" => Ok(Action::CloseSearchOverlay),
            "show_search_overlay" => Ok(Action::ShowSearchOverlay),
            "search_overlay_submit" => Ok(Action::SearchOverlaySubmit),
            "search_overlay_confirm" => Ok(Action::SearchOverlayConfirm),
            "show_file_upload_explorer" => Ok(Action::ShowFileUploadExplorer),
            "hide_file_upload_explorer" => Ok(Action::HideFileUploadExplorer),
            "load_pinned_messages" => Ok(Action::LoadPinnedMessages),
            "toggle_chat_pinned_bar" | "show_pinned_messages_popup" => {
                Ok(Action::ShowPinnedMessagesPopup)
            }
            "hide_pinned_messages_popup" => Ok(Action::HidePinnedMessagesPopup),
            "pinned_popup_previous" => Ok(Action::PinnedPopupPrevious),
            "pinned_popup_next" => Ok(Action::PinnedPopupNext),
            "pinned_popup_confirm_jump" => Ok(Action::PinnedPopupConfirmJump),
            "pinned_popup_view_media" => Ok(Action::PinnedPopupViewMedia),
            "pinned_popup_open_media" => Ok(Action::PinnedPopupOpenMedia),
            _ => Err(AppError::InvalidAction(s.to_string())),
        }
    }
}
