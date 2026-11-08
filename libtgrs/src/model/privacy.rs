//! Privacy settings: who sees your last seen time, phone number, profile
//! photo and more, with people and groups as exceptions.

use tdlib_rs::enums::{UserPrivacySetting, UserPrivacySettingRule};
use tdlib_rs::types::{
    UserPrivacySettingRuleAllowChatMembers, UserPrivacySettingRuleAllowUsers,
    UserPrivacySettingRuleRestrictChatMembers, UserPrivacySettingRuleRestrictUsers,
    UserPrivacySettingRules,
};

/// A setting the privacy popup changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrivacySetting {
    LastSeen,
    PhoneNumber,
    FindByPhoneNumber,
    ProfilePhoto,
    Bio,
    Birthday,
    ForwardedMessages,
    Calls,
    ChatInvites,
}

impl PrivacySetting {
    pub const ALL: [Self; 9] = [
        Self::LastSeen,
        Self::PhoneNumber,
        Self::FindByPhoneNumber,
        Self::ProfilePhoto,
        Self::Bio,
        Self::Birthday,
        Self::ForwardedMessages,
        Self::Calls,
        Self::ChatInvites,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::LastSeen => "Last seen & online",
            Self::PhoneNumber => "Phone number",
            Self::FindByPhoneNumber => "Find me by number",
            Self::ProfilePhoto => "Profile photo",
            Self::Bio => "Bio",
            Self::Birthday => "Birthday",
            Self::ForwardedMessages => "Link in forwards",
            Self::Calls => "Calls",
            Self::ChatInvites => "Invites to groups",
        }
    }

    /// The audiences Telegram takes for it: no one can't be kept from
    /// finding you by your number.
    pub fn audiences(self) -> &'static [Audience] {
        match self {
            Self::FindByPhoneNumber => &[Audience::Everybody, Audience::Contacts],
            _ => &[Audience::Everybody, Audience::Contacts, Audience::Nobody],
        }
    }

    /// Whether people and groups can be exceptions to it.
    pub fn has_exceptions(self) -> bool {
        self != Self::FindByPhoneNumber
    }
}

impl From<PrivacySetting> for UserPrivacySetting {
    fn from(setting: PrivacySetting) -> Self {
        match setting {
            PrivacySetting::LastSeen => Self::ShowStatus,
            PrivacySetting::PhoneNumber => Self::ShowPhoneNumber,
            PrivacySetting::FindByPhoneNumber => Self::AllowFindingByPhoneNumber,
            PrivacySetting::ProfilePhoto => Self::ShowProfilePhoto,
            PrivacySetting::Bio => Self::ShowBio,
            PrivacySetting::Birthday => Self::ShowBirthdate,
            PrivacySetting::ForwardedMessages => Self::ShowLinkInForwardedMessages,
            PrivacySetting::Calls => Self::AllowCalls,
            PrivacySetting::ChatInvites => Self::AllowChatInvites,
        }
    }
}

/// Who a setting is for, apart from its exceptions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Audience {
    #[default]
    Everybody,
    Contacts,
    Nobody,
}

impl Audience {
    pub fn label(self) -> &'static str {
        match self {
            Self::Everybody => "Everybody",
            Self::Contacts => "My contacts",
            Self::Nobody => "Nobody",
        }
    }

    /// The next of `allowed`, wrapping around.
    pub fn next(self, allowed: &[Audience]) -> Audience {
        let index = allowed.iter().position(|a| *a == self).unwrap_or(0);
        allowed[(index + 1) % allowed.len()]
    }
}

/// How a person is treated by a setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exception {
    /// Always allowed, whatever the audience.
    Always,
    /// Never allowed.
    Never,
}

/// A setting's rules, as an audience and exceptions.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PrivacyRules {
    pub audience: Audience,
    pub always_users: Vec<i64>,
    pub always_chats: Vec<i64>,
    pub never_users: Vec<i64>,
    pub never_chats: Vec<i64>,
    /// Rules about bots and Premium users, kept as they are.
    pub other: Vec<UserPrivacySettingRule>,
}

// TDLib's rules hold only ids, so equality is total
impl Eq for PrivacyRules {}

impl PrivacyRules {
    pub fn new(td: &UserPrivacySettingRules) -> Self {
        let mut rules = PrivacyRules {
            audience: Audience::Nobody,
            ..PrivacyRules::default()
        };
        let mut everybody = false;
        let mut contacts = false;
        for rule in &td.rules {
            match rule {
                UserPrivacySettingRule::AllowAll => everybody = true,
                UserPrivacySettingRule::AllowContacts => contacts = true,
                UserPrivacySettingRule::RestrictAll => {}
                UserPrivacySettingRule::AllowUsers(r) => {
                    rules.always_users.extend(&r.user_ids);
                }
                UserPrivacySettingRule::AllowChatMembers(r) => {
                    rules.always_chats.extend(&r.chat_ids);
                }
                UserPrivacySettingRule::RestrictUsers(r) => {
                    rules.never_users.extend(&r.user_ids);
                }
                UserPrivacySettingRule::RestrictChatMembers(r) => {
                    rules.never_chats.extend(&r.chat_ids);
                }
                other => rules.other.push(other.clone()),
            }
        }
        rules.audience = if everybody {
            Audience::Everybody
        } else if contacts {
            Audience::Contacts
        } else {
            Audience::Nobody
        };
        rules
    }

    /// TDLib's rules: they're checked in order, so exceptions come before
    /// the audience.
    pub fn to_td(&self) -> UserPrivacySettingRules {
        let mut rules = Vec::new();
        if !self.never_users.is_empty() {
            rules.push(UserPrivacySettingRule::RestrictUsers(
                UserPrivacySettingRuleRestrictUsers {
                    user_ids: self.never_users.clone(),
                },
            ));
        }
        if !self.never_chats.is_empty() {
            rules.push(UserPrivacySettingRule::RestrictChatMembers(
                UserPrivacySettingRuleRestrictChatMembers {
                    chat_ids: self.never_chats.clone(),
                },
            ));
        }
        if !self.always_users.is_empty() {
            rules.push(UserPrivacySettingRule::AllowUsers(
                UserPrivacySettingRuleAllowUsers {
                    user_ids: self.always_users.clone(),
                },
            ));
        }
        if !self.always_chats.is_empty() {
            rules.push(UserPrivacySettingRule::AllowChatMembers(
                UserPrivacySettingRuleAllowChatMembers {
                    chat_ids: self.always_chats.clone(),
                },
            ));
        }
        rules.extend(self.other.iter().cloned());
        rules.push(match self.audience {
            Audience::Everybody => UserPrivacySettingRule::AllowAll,
            Audience::Contacts => UserPrivacySettingRule::AllowContacts,
            Audience::Nobody => UserPrivacySettingRule::RestrictAll,
        });
        UserPrivacySettingRules { rules }
    }

    /// How `user_id` is an exception; None when they follow the audience.
    pub fn exception(&self, user_id: i64) -> Option<Exception> {
        if self.always_users.contains(&user_id) {
            Some(Exception::Always)
        } else if self.never_users.contains(&user_id) {
            Some(Exception::Never)
        } else {
            None
        }
    }

    /// Makes `user_id` an exception, or follow the audience again with None.
    pub fn set_exception(&mut self, user_id: i64, exception: Option<Exception>) {
        self.always_users.retain(|id| *id != user_id);
        self.never_users.retain(|id| *id != user_id);
        match exception {
            Some(Exception::Always) => self.always_users.push(user_id),
            Some(Exception::Never) => self.never_users.push(user_id),
            None => {}
        }
    }

    /// The audience with its exceptions, e.g. `My contacts (+2, −1)`.
    pub fn summary(&self) -> String {
        let always = self.always_users.len() + self.always_chats.len();
        let never = self.never_users.len() + self.never_chats.len();
        let mut counts = Vec::new();
        if always > 0 {
            counts.push(format!("+{always}"));
        }
        if never > 0 {
            counts.push(format!("−{never}"));
        }
        if counts.is_empty() {
            self.audience.label().to_owned()
        } else {
            format!("{} ({})", self.audience.label(), counts.join(", "))
        }
    }
}

/// What the privacy popup shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PrivacySettings {
    pub rules: Vec<(PrivacySetting, PrivacyRules)>,
    /// Others see when you read their messages in private chats; turning
    /// it off hides theirs from you too (without Premium).
    pub show_read_date: bool,
}

impl PrivacySettings {
    pub fn rules(&self, setting: PrivacySetting) -> Option<&PrivacyRules> {
        self.rules
            .iter()
            .find(|(s, _)| *s == setting)
            .map(|(_, rules)| rules)
    }

    pub fn rules_mut(&mut self, setting: PrivacySetting) -> Option<&mut PrivacyRules> {
        self.rules
            .iter_mut()
            .find(|(s, _)| *s == setting)
            .map(|(_, rules)| rules)
    }
}
