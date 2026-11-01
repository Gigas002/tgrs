//! Blocking people and reporting chats or messages.

use tdlib_rs::enums::ReportChatResult;

use super::td_enums::TdMessageSender;

/// Someone on your block list, with the name to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockedSender {
    pub sender: TdMessageSender,
    pub name: String,
}

/// One of the reasons Telegram offers for a report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportOption {
    /// Sent back to pick the option; opaque.
    pub id: String,
    pub text: String,
}

/// Where a report stands after a step: Telegram asks for a reason (maybe
/// several times, each narrowing the last), then maybe a comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportStep {
    /// The report was sent.
    Done,
    /// Pick one of `options`.
    ChooseOption {
        title: String,
        options: Vec<ReportOption>,
    },
    /// Write a comment for `option_id`, which may be left empty if `optional`.
    WriteText { option_id: String, optional: bool },
    /// The chat can only be reported along with some of its messages.
    NeedsMessages,
}

impl From<ReportChatResult> for ReportStep {
    fn from(result: ReportChatResult) -> Self {
        match result {
            ReportChatResult::Ok => Self::Done,
            ReportChatResult::OptionRequired(required) => Self::ChooseOption {
                title: required.title,
                options: required
                    .options
                    .into_iter()
                    .map(|option| ReportOption {
                        id: option.id,
                        text: option.text,
                    })
                    .collect(),
            },
            ReportChatResult::TextRequired(required) => Self::WriteText {
                option_id: required.option_id,
                optional: required.is_optional,
            },
            ReportChatResult::MessagesRequired => Self::NeedsMessages,
        }
    }
}
