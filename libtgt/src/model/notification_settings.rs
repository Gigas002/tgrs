//! Telegram's notification settings: the defaults for private chats, groups
//! and channels, and a chat's own overrides.

use tdlib_rs::enums::NotificationSettingsScope;
use tdlib_rs::types::{ChatNotificationSettings, ScopeNotificationSettings};

use super::chat::ChatKind;

/// `mute_for` that TDLib takes as forever (more than a year, in seconds).
pub const MUTE_FOREVER: i32 = i32::MAX;

/// The kinds of chats that share default settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotificationScope {
    /// Private and secret chats.
    PrivateChats,
    /// Basic groups and supergroups.
    Groups,
    Channels,
}

impl NotificationScope {
    pub const ALL: [Self; 3] = [Self::PrivateChats, Self::Groups, Self::Channels];

    /// The scope whose defaults a chat of `kind` follows.
    pub fn of(kind: ChatKind) -> Self {
        match kind {
            ChatKind::Group => Self::Groups,
            ChatKind::Channel => Self::Channels,
            ChatKind::Private
            | ChatKind::Bot
            | ChatKind::Saved
            | ChatKind::Secret
            | ChatKind::Archive => Self::PrivateChats,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::PrivateChats => "Private chats",
            Self::Groups => "Groups",
            Self::Channels => "Channels",
        }
    }

    /// The options that mean something for these chats: there are no
    /// mentions in private chats or channels, and no pinned messages to
    /// announce in private chats.
    pub fn options(self) -> &'static [NotificationOption] {
        use NotificationOption::*;
        match self {
            Self::PrivateChats => &[Alerts, Preview],
            Self::Groups => &[Alerts, Preview, Mentions, PinnedMessages],
            Self::Channels => &[Alerts, Preview, PinnedMessages],
        }
    }
}

impl From<NotificationScope> for NotificationSettingsScope {
    fn from(scope: NotificationScope) -> Self {
        match scope {
            NotificationScope::PrivateChats => Self::PrivateChats,
            NotificationScope::Groups => Self::GroupChats,
            NotificationScope::Channels => Self::ChannelChats,
        }
    }
}

/// One setting that can be turned on or off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotificationOption {
    /// Notifications at all: off means muted.
    Alerts,
    /// The message text in notifications.
    Preview,
    /// Notifications for mentions and replies even when muted.
    Mentions,
    /// Notifications for newly pinned messages.
    PinnedMessages,
}

impl NotificationOption {
    pub fn label(self) -> &'static str {
        match self {
            Self::Alerts => "Notifications",
            Self::Preview => "Message preview",
            Self::Mentions => "Mentions and replies",
            Self::PinnedMessages => "Pinned messages",
        }
    }
}

/// What a scope or chat's settings are set for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotificationTarget {
    Scope(NotificationScope),
    Chat(i64),
}

/// The defaults of a scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScopeSettings {
    pub scope: NotificationScope,
    pub alerts: bool,
    pub preview: bool,
    pub mentions: bool,
    pub pinned_messages: bool,
}

impl ScopeSettings {
    pub fn new(scope: NotificationScope, td: &ScopeNotificationSettings) -> Self {
        ScopeSettings {
            scope,
            alerts: td.mute_for == 0,
            preview: td.show_preview,
            mentions: !td.disable_mention_notifications,
            pinned_messages: !td.disable_pinned_message_notifications,
        }
    }

    pub fn get(&self, option: NotificationOption) -> bool {
        match option {
            NotificationOption::Alerts => self.alerts,
            NotificationOption::Preview => self.preview,
            NotificationOption::Mentions => self.mentions,
            NotificationOption::PinnedMessages => self.pinned_messages,
        }
    }

    pub fn set(&mut self, option: NotificationOption, on: bool) {
        match option {
            NotificationOption::Alerts => self.alerts = on,
            NotificationOption::Preview => self.preview = on,
            NotificationOption::Mentions => self.mentions = on,
            NotificationOption::PinnedMessages => self.pinned_messages = on,
        }
    }
}

/// Sets `option` in TDLib's scope settings, keeping the rest.
pub fn apply_scope_option(
    td: &mut ScopeNotificationSettings,
    option: NotificationOption,
    on: bool,
) {
    match option {
        NotificationOption::Alerts => td.mute_for = if on { 0 } else { MUTE_FOREVER },
        NotificationOption::Preview => td.show_preview = on,
        NotificationOption::Mentions => td.disable_mention_notifications = !on,
        NotificationOption::PinnedMessages => td.disable_pinned_message_notifications = !on,
    }
}

/// A chat's own settings; None for each one that follows its scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatSettings {
    pub chat_id: i64,
    pub title: String,
    pub scope: NotificationScope,
    pub alerts: Option<bool>,
    pub preview: Option<bool>,
    pub mentions: Option<bool>,
    pub pinned_messages: Option<bool>,
}

impl ChatSettings {
    pub fn new(chat_id: i64, title: String, kind: ChatKind, td: &ChatNotificationSettings) -> Self {
        let own = |use_default: bool, value: bool| (!use_default).then_some(value);
        ChatSettings {
            chat_id,
            title,
            scope: NotificationScope::of(kind),
            alerts: own(td.use_default_mute_for, td.mute_for == 0),
            preview: own(td.use_default_show_preview, td.show_preview),
            mentions: own(
                td.use_default_disable_mention_notifications,
                !td.disable_mention_notifications,
            ),
            pinned_messages: own(
                td.use_default_disable_pinned_message_notifications,
                !td.disable_pinned_message_notifications,
            ),
        }
    }

    pub fn get(&self, option: NotificationOption) -> Option<bool> {
        match option {
            NotificationOption::Alerts => self.alerts,
            NotificationOption::Preview => self.preview,
            NotificationOption::Mentions => self.mentions,
            NotificationOption::PinnedMessages => self.pinned_messages,
        }
    }

    pub fn set(&mut self, option: NotificationOption, value: Option<bool>) {
        match option {
            NotificationOption::Alerts => self.alerts = value,
            NotificationOption::Preview => self.preview = value,
            NotificationOption::Mentions => self.mentions = value,
            NotificationOption::PinnedMessages => self.pinned_messages = value,
        }
    }
}

/// Sets `option` in TDLib's chat settings, keeping the rest; None goes back
/// to the scope's default.
pub fn apply_chat_option(
    td: &mut ChatNotificationSettings,
    option: NotificationOption,
    value: Option<bool>,
) {
    let use_default = value.is_none();
    let on = value.unwrap_or_default();
    match option {
        NotificationOption::Alerts => {
            td.use_default_mute_for = use_default;
            td.mute_for = if on || use_default { 0 } else { MUTE_FOREVER };
        }
        NotificationOption::Preview => {
            td.use_default_show_preview = use_default;
            td.show_preview = on;
        }
        NotificationOption::Mentions => {
            td.use_default_disable_mention_notifications = use_default;
            td.disable_mention_notifications = !on;
        }
        NotificationOption::PinnedMessages => {
            td.use_default_disable_pinned_message_notifications = use_default;
            td.disable_pinned_message_notifications = !on;
        }
    }
}

/// The settings the notification popup shows: every scope's, and the open
/// chat's when one is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationSettings {
    pub scopes: Vec<ScopeSettings>,
    pub chat: Option<ChatSettings>,
}

impl NotificationSettings {
    pub fn scope(&self, scope: NotificationScope) -> Option<&ScopeSettings> {
        self.scopes.iter().find(|s| s.scope == scope)
    }

    /// What a chat's option is in effect: its own value, else its scope's.
    pub fn effective(&self, chat: &ChatSettings, option: NotificationOption) -> bool {
        chat.get(option)
            .or_else(|| self.scope(chat.scope).map(|s| s.get(option)))
            .unwrap_or(true)
    }
}
