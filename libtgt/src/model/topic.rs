use tdlib_rs::enums::MessageTopic;
use tdlib_rs::types::{ChatNotificationSettings, ForumTopic, ForumTopicInfo, UpdateForumTopic};

use super::message::MessageEntry;

/// One row of a forum's topic list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TopicEntry {
    pub chat_id: i64,
    pub topic_id: i32,
    pub name: String,
    /// Orders the topic list, the highest first.
    pub order: i64,
    pub last_message: Option<MessageEntry>,
    /// Whether this topic is pinned in the topic list.
    pub is_pinned: bool,
    /// The General topic, where messages without a topic go.
    pub is_general: bool,
    /// Only admins and the topic's creator can write in a closed topic.
    pub is_closed: bool,
    /// The General topic can be hidden above the topic list.
    pub is_hidden: bool,
    pub unread_count: i32,
    /// Unread messages that mention you.
    pub unread_mention_count: i32,
    /// Identifier of the last read incoming message.
    pub last_read_inbox_message_id: i64,
    /// Notifications are turned off.
    pub is_muted: bool,
}

impl TopicEntry {
    /// The topic `info` describes changed; the rest is kept.
    pub fn set_info(&mut self, info: &ForumTopicInfo) {
        self.name = info.name.clone();
        self.is_general = info.is_general;
        self.is_closed = info.is_closed;
        self.is_hidden = info.is_hidden;
    }

    /// Applies an `updateForumTopic`.
    pub fn apply(&mut self, update: &UpdateForumTopic) {
        self.is_pinned = update.is_pinned;
        self.unread_mention_count = update.unread_mention_count;
        self.last_read_inbox_message_id = update.last_read_inbox_message_id;
        self.is_muted = is_muted(&update.notification_settings);
    }

    /// The icon shown before the topic's name.
    pub fn icon(&self) -> &'static str {
        if self.is_general { "#" } else { "💬" }
    }

    /// Whether `topic` (a message's topic) is this one. Messages of the
    /// General topic may come without one.
    pub fn contains(&self, topic: Option<&MessageTopic>) -> bool {
        match topic {
            Some(MessageTopic::Forum(forum)) => forum.forum_topic_id == self.topic_id,
            None => self.is_general,
            Some(_) => false,
        }
    }
}

impl From<&ForumTopic> for TopicEntry {
    fn from(topic: &ForumTopic) -> Self {
        let mut entry = Self {
            chat_id: topic.info.chat_id,
            topic_id: topic.info.forum_topic_id,
            order: topic.order,
            last_message: topic.last_message.as_ref().map(MessageEntry::from),
            is_pinned: topic.is_pinned,
            unread_count: topic.unread_count,
            unread_mention_count: topic.unread_mention_count,
            last_read_inbox_message_id: topic.last_read_inbox_message_id,
            is_muted: is_muted(&topic.notification_settings),
            ..Self::default()
        };
        entry.set_info(&topic.info);
        entry
    }
}

fn is_muted(settings: &ChatNotificationSettings) -> bool {
    !settings.use_default_mute_for && settings.mute_for > 0
}
