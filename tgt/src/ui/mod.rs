pub const SMALL_AREA_WIDTH: u16 = 100;
pub const SMALL_AREA_HEIGHT: u16 = 20;
pub const MAX_CHAT_LIST_SIZE: u16 = 50;
pub const MIN_CHAT_LIST_SIZE: u16 = 10;
/// The chat list and the topic list together take at most this much of the width, in percent.
pub const MAX_SIDE_PANES_SIZE: u16 = 70;
pub const MAX_PROMPT_SIZE: u16 = 20;
pub const MIN_PROMPT_SIZE: u16 = 3;

pub mod archive_list_window;
pub mod blocked_list;
pub mod chat_actions;
pub mod chat_list_window;
pub mod chat_window;
pub mod command_guide;
pub mod component_name;
pub mod component_traits;
pub mod core_window;
pub mod edit_profile;
pub mod file_upload_explorer;
pub mod find_chat;
pub mod leader_menu;
pub mod message;
pub mod message_actions;
pub mod pane;
pub mod picker;
pub mod pinned_messages_popup;
pub mod popup_list;
#[cfg(feature = "media-preview")]
pub mod preview;
pub mod profile_popup;
pub mod prompt_window;
pub mod reply_message;
pub mod report;
pub mod search_overlay;
pub mod sessions;
pub mod status_bar;
pub mod sticker_picker;
pub mod terminal;
pub mod text_input;
pub mod title_bar;
pub mod topic_list_window;
mod tui;

pub use tui::Tui;

#[cfg(test)]
mod tests;
