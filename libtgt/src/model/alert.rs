/// A new message TDLib wants the user to be notified about.
///
/// TDLib already applied the chat's notification settings (muted chats,
/// mention-only groups, already read messages), so every alert is meant to be shown.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MessageAlert {
    pub chat_id: i64,
    pub message_id: i64,
    /// Title of the chat the message arrived in.
    pub chat_title: String,
    /// Who sent the message, when it differs from the chat (groups).
    pub sender_name: Option<String>,
    /// The message text, or `None` when the chat hides message previews.
    pub text: Option<String>,
    /// The message was sent without sound.
    pub is_silent: bool,
}

impl MessageAlert {
    /// The notification title: `Sender in Chat`, or just the chat.
    pub fn title(&self) -> String {
        match &self.sender_name {
            Some(sender) => format!("{sender} in {}", self.chat_title),
            None => self.chat_title.clone(),
        }
    }
}
