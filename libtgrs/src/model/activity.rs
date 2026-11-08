//! What others are doing in a chat right now: typing, recording, choosing
//! a sticker. Telegram repeats an action every few seconds while it lasts.

use std::time::{Duration, Instant};

use tdlib_rs::enums::{ChatAction, MessageTopic};

use super::td_enums::TdMessageSender;

/// How long an action is shown without being repeated.
pub const ACTIVITY_TIMEOUT: Duration = Duration::from_secs(6);

/// The kinds of actions told apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActivityKind {
    Typing,
    RecordingVoice,
    RecordingVideo,
    SendingFile,
    ChoosingSticker,
    Other,
}

impl ActivityKind {
    /// None for a cancelled action.
    pub fn of(action: &ChatAction) -> Option<Self> {
        Some(match action {
            ChatAction::Cancel => return None,
            ChatAction::Typing => Self::Typing,
            ChatAction::RecordingVoiceNote => Self::RecordingVoice,
            ChatAction::RecordingVideo | ChatAction::RecordingVideoNote => Self::RecordingVideo,
            ChatAction::UploadingVideo(_)
            | ChatAction::UploadingVoiceNote(_)
            | ChatAction::UploadingPhoto(_)
            | ChatAction::UploadingDocument(_)
            | ChatAction::UploadingVideoNote(_) => Self::SendingFile,
            ChatAction::ChoosingSticker => Self::ChoosingSticker,
            _ => Self::Other,
        })
    }

    /// E.g. `typing`, said of one person.
    fn one(self) -> &'static str {
        match self {
            Self::Typing => "is typing",
            Self::RecordingVoice => "is recording a voice message",
            Self::RecordingVideo => "is recording a video",
            Self::SendingFile => "is sending a file",
            Self::ChoosingSticker => "is choosing a sticker",
            Self::Other => "is busy",
        }
    }

    /// The same, of a private chat's other party, without a name.
    fn alone(self) -> &'static str {
        self.one().trim_start_matches("is ")
    }
}

/// Someone's ongoing action in a chat.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatActivity {
    pub sender: TdMessageSender,
    pub topic: Option<MessageTopic>,
    pub kind: ActivityKind,
    /// When it's no longer shown unless repeated.
    pub until: Instant,
}

/// The line under a chat's title, e.g. `Ferris is typing…`; `named` is
/// false in private chats, where it's only `typing…`. `names` are the
/// people acting, with what they do, latest first.
pub fn activity_text(names: &[(String, ActivityKind)], named: bool) -> Option<String> {
    let (first, kind) = names.first()?;
    if !named {
        return Some(format!("{}…", kind.alone()));
    }
    Some(match names.len() {
        1 => format!("{first} {}…", kind.one()),
        2 => {
            let second = &names[1].0;
            if names[1].1 == *kind && *kind == ActivityKind::Typing {
                format!("{first} and {second} are typing…")
            } else {
                format!("{first} and {second} are busy…")
            }
        }
        n => format!("{first} and {} others are busy…", n - 1),
    })
}
