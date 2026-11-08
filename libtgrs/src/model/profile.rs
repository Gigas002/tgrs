//! What the profile popup shows of a chat: a user's or a group's details.

use tdlib_rs::{enums::UserStatus, types::Birthdate};

use super::{chat::ChatKind, message::format_timestamp};

/// Whose full info a profile needs from TDLib.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileSource {
    User(i64),
    BasicGroup(i64),
    Supergroup(i64),
}

/// A chat's profile: a person's or bot's for private and secret chats (yours
/// for Saved Messages), the group's or channel's otherwise.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Profile {
    pub chat_id: i64,
    pub kind: ChatKind,
    /// Whether this is your own profile.
    pub is_me: bool,
    /// A user's full name, or a group's title.
    pub name: String,
    /// Active usernames, without `@`.
    pub usernames: Vec<String>,
    /// Without `+`; empty when hidden.
    pub phone_number: String,
    /// A user's bio or a bot's or group's description.
    pub bio: String,
    pub birthdate: Option<String>,
    /// When a user was last seen, or how many members a group has.
    pub status: String,
    /// The user's ID for users, the chat's otherwise.
    pub id: i64,
    /// Whether the full info (bio, description) is known yet.
    pub is_loaded: bool,
}

impl Profile {
    /// The rows of the profile as label and value, leaving out empty ones.
    pub fn fields(&self) -> Vec<(&'static str, String)> {
        let is_user = matches!(
            self.kind,
            ChatKind::Private | ChatKind::Bot | ChatKind::Saved | ChatKind::Secret
        );
        let usernames = self
            .usernames
            .iter()
            .map(|name| format!("@{name}"))
            .collect::<Vec<_>>()
            .join(", ");
        let phone = if self.phone_number.is_empty() {
            String::new()
        } else {
            format!("+{}", self.phone_number)
        };
        let bio_label = match self.kind {
            ChatKind::Bot => "About",
            _ if is_user => "Bio",
            _ => "Description",
        };
        let status_label = match self.kind {
            ChatKind::Channel => "Subscribers",
            ChatKind::Group => "Members",
            // Not "Last seen online"
            _ if self.status == "online" => "Status",
            _ => "Last seen",
        };
        [
            ("Username", usernames),
            ("Phone", phone),
            (bio_label, self.bio.clone()),
            ("Birthday", self.birthdate.clone().unwrap_or_default()),
            (status_label, self.status.clone()),
            ("ID", self.id.to_string()),
        ]
        .into_iter()
        .filter(|(_, value)| !value.is_empty())
        .collect()
    }
}

/// When a user was last seen, e.g. `online` or `recently`; empty if unknown.
pub fn user_status_text(status: &UserStatus) -> String {
    match status {
        UserStatus::Empty => String::new(),
        UserStatus::Online(_) => "online".to_owned(),
        UserStatus::Offline(offline) => format_timestamp(offline.was_online),
        UserStatus::Recently(_) => "recently".to_owned(),
        UserStatus::LastWeek(_) => "within a week".to_owned(),
        UserStatus::LastMonth(_) => "within a month".to_owned(),
    }
}

/// A birthday as `day.month` or `day.month.year`.
pub fn birthdate_text(date: &Birthdate) -> String {
    if date.year > 0 {
        format!("{:02}.{:02}.{}", date.day, date.month, date.year)
    } else {
        format!("{:02}.{:02}", date.day, date.month)
    }
}

/// How many members a group has; empty if unknown.
pub fn member_count_text(count: i32) -> String {
    if count > 0 {
        count.to_string()
    } else {
        String::new()
    }
}
