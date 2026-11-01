//! In-memory Telegram state: everything TDLib told us, plus the open chat session.
//!
//! The client's update loop writes here; the UI reads through the accessors.

use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64, Ordering};
use std::sync::{Mutex, MutexGuard};

use tdlib_rs::{
    enums::{ChatList, ChatType, MessageTopic, UserStatus, UserType},
    types::{
        BasicGroup, BasicGroupFullInfo, Chat, ForumTopicInfo, Message, MessageTopicForum,
        SecretChat, Supergroup, SupergroupFullInfo, UpdateForumTopic, User, UserFullInfo,
    },
};

use crate::model::{
    ChatKind, ChatListEntry, ChatState, MessageAlert, MessageEntry, OrderedChat, Profile,
    ProfileSource, TdMessageSender, TopicEntry, format_timestamp,
    ids::{AtomicChatId, AtomicMessageId, ChatId, MessageId},
    profile::{birthdate_text, member_count_text, user_status_text},
};

pub mod open_chat;

#[cfg(test)]
mod tests;

pub use open_chat::OpenChatMessageStore;

/// The name of the row standing for the archive in the main chat list.
pub const ARCHIVE_NAME: &str = "Archived chats";
/// Archived chats named in the archive row's preview at most.
const ARCHIVE_PREVIEW_CHATS: usize = 10;

#[derive(Debug)]
pub struct Store {
    users: Mutex<HashMap<i64, User>>,
    basic_groups: Mutex<HashMap<i64, BasicGroup>>,
    supergroups: Mutex<HashMap<i64, Supergroup>>,
    secret_chats: Mutex<HashMap<i32, SecretChat>>,
    chats: Mutex<HashMap<i64, Chat>>,

    // Ordered
    chats_index: Mutex<BTreeSet<OrderedChat>>,
    /// Chats of the Archive chat list, ordered like `chats_index`.
    archive_index: Mutex<BTreeSet<OrderedChat>>,

    users_full_info: Mutex<HashMap<i64, UserFullInfo>>,
    basic_groups_full_info: Mutex<HashMap<i64, BasicGroupFullInfo>>,
    supergroups_full_info: Mutex<HashMap<i64, SupergroupFullInfo>>,

    me: AtomicI64,
    open_chat_id: AtomicChatId,
    /// Message cache and view window for the open chat.
    open_chat_messages: Mutex<OpenChatMessageStore>,
    /// Pinned messages for the open chat (newest pin first). Cleared when switching chats
    /// (see chat list) or replaced by [`Self::set_open_chat_pinned`]. Not cleared when only the
    /// message cache is cleared (e.g. jump-to-message reload).
    open_chat_pinned: Mutex<Vec<MessageEntry>>,
    open_chat_user: Mutex<Option<User>>,
    /// Whether the open chat is a forum, whose messages are in topics.
    open_chat_is_forum: AtomicBool,
    /// Topics of the open chat when it is a forum, in topic list order.
    open_chat_topics: Mutex<Vec<TopicEntry>>,
    /// The open topic of a forum chat (0 = none); its messages are the open chat's.
    open_topic_id: AtomicI32,

    last_acknowledged_message_id: AtomicI64,

    /// The message id from which to start loading the chat history (set by backend when appending).
    from_message_id: AtomicI64,

    /// True while a "load more history" request is in flight; avoids duplicate requests.
    history_loading: AtomicBool,

    /// After JumpToMessage completes, ChatWindow should select this message_id (MessageId::NONE = none).
    jump_target_message_id: AtomicMessageId,

    /// reply message id (MessageId::NONE = no reply)
    reply_message_id: AtomicMessageId,
    /// reply message text
    reply_message_text: Mutex<String>,
}

impl Store {
    pub fn users(&self) -> MutexGuard<'_, HashMap<i64, User>> {
        self.users.lock().unwrap()
    }
    pub fn basic_groups(&self) -> MutexGuard<'_, HashMap<i64, BasicGroup>> {
        self.basic_groups.lock().unwrap()
    }
    pub fn supergroups(&self) -> MutexGuard<'_, HashMap<i64, Supergroup>> {
        self.supergroups.lock().unwrap()
    }
    pub fn secret_chats(&self) -> MutexGuard<'_, HashMap<i32, SecretChat>> {
        self.secret_chats.lock().unwrap()
    }
    pub fn chats(&self) -> MutexGuard<'_, HashMap<i64, Chat>> {
        self.chats.lock().unwrap()
    }
    pub fn chats_index(&self) -> MutexGuard<'_, BTreeSet<OrderedChat>> {
        self.chats_index.lock().unwrap()
    }
    pub fn archive_index(&self) -> MutexGuard<'_, BTreeSet<OrderedChat>> {
        self.archive_index.lock().unwrap()
    }
    pub fn users_full_info(&self) -> MutexGuard<'_, HashMap<i64, UserFullInfo>> {
        self.users_full_info.lock().unwrap()
    }
    pub fn basic_groups_full_info(&self) -> MutexGuard<'_, HashMap<i64, BasicGroupFullInfo>> {
        self.basic_groups_full_info.lock().unwrap()
    }
    pub fn supergroups_full_info(&self) -> MutexGuard<'_, HashMap<i64, SupergroupFullInfo>> {
        self.supergroups_full_info.lock().unwrap()
    }
    /// Get the open chat ID (ChatId::NONE if no chat is open).
    pub fn open_chat_id(&self) -> ChatId {
        self.open_chat_id.load(Ordering::Relaxed)
    }

    /// Get the open chat ID as i64 (0 if no chat is open). For backward compatibility.
    pub fn open_chat_id_i64(&self) -> i64 {
        self.open_chat_id().as_i64()
    }
    pub fn open_chat_messages(&self) -> MutexGuard<'_, OpenChatMessageStore> {
        self.open_chat_messages.lock().unwrap()
    }
    pub fn me(&self) -> i64 {
        self.me.load(Ordering::Relaxed)
    }
    pub fn from_message_id(&self) -> i64 {
        self.from_message_id.load(Ordering::Relaxed)
    }
    pub fn open_chat_user(&self) -> MutexGuard<'_, Option<User>> {
        self.open_chat_user.lock().unwrap()
    }
    pub fn last_acknowledged_message_id(&self) -> i64 {
        self.last_acknowledged_message_id.load(Ordering::Relaxed)
    }

    pub fn set_open_chat_user(&self, user: Option<User>) {
        *self.open_chat_user() = user;
    }

    /// Set the open chat ID (use ChatId::NONE to clear).
    pub fn set_open_chat_id(&self, chat_id: ChatId) {
        self.open_chat_id.store(chat_id, Ordering::Relaxed);
    }

    /// Set the open chat ID from i64 (0 = no chat). For backward compatibility.
    pub fn set_open_chat_id_i64(&self, chat_id: i64) {
        self.set_open_chat_id(ChatId::new(chat_id));
    }

    pub fn clear_open_chat_messages(&self) {
        let open_id = self.open_chat_id.load(Ordering::Relaxed);
        tracing::debug!(
            open_chat_id = open_id.as_i64(),
            "clearing open chat messages"
        );
        self.open_chat_messages().clear();
    }

    pub fn set_from_message_id(&self, from_message_id: i64) {
        self.from_message_id
            .store(from_message_id, Ordering::Relaxed);
    }

    // ----- Read-only API for UI -----

    /// Ordered message IDs for the open chat (oldest to newest). UI uses this to build the list.
    pub fn ordered_message_ids(&self) -> Vec<i64> {
        self.open_chat_messages().ordered_message_ids()
    }

    /// Single-lock snapshot of all messages for the open chat (oldest to newest). Use in UI draw
    /// to avoid TOCTOU: ordered_message_ids() then get_message(id) can see a cleared store between calls.
    pub fn ordered_messages_snapshot(&self) -> Vec<MessageEntry> {
        self.open_chat_messages().ordered_messages()
    }

    /// Snapshot of pinned messages for the open chat (newest first, as set by the backend).
    pub fn open_chat_pinned_snapshot(&self) -> Vec<MessageEntry> {
        self.open_chat_pinned.lock().unwrap().clone()
    }

    /// Replace pinned messages for the open chat (backend / load task).
    pub fn set_open_chat_pinned(&self, messages: Vec<MessageEntry>) {
        *self.open_chat_pinned.lock().unwrap() = messages;
    }

    /// Get a message by ID (clone). UI read-only.
    pub fn get_message(&self, id: i64) -> Option<MessageEntry> {
        self.open_chat_messages().get_message(id)
    }

    /// Oldest loaded message ID; None if empty.
    pub fn oldest_message_id(&self) -> Option<i64> {
        self.open_chat_messages().oldest_message_id()
    }

    /// Newest loaded message ID; None if empty.
    pub fn newest_message_id(&self) -> Option<i64> {
        self.open_chat_messages().newest_message_id()
    }

    /// True while a "load more history" request is in flight.
    pub fn is_history_loading(&self) -> bool {
        self.history_loading.load(Ordering::Acquire)
    }

    /// Set history loading flag (backend/task only).
    pub fn set_history_loading(&self, value: bool) {
        self.history_loading.store(value, Ordering::Release);
    }

    /// Jump target message ID (set by run loop after JumpToMessage; ChatWindow selects it).
    /// Returns MessageId::NONE if no jump target is set.
    pub fn jump_target_message_id(&self) -> MessageId {
        self.jump_target_message_id.load(Ordering::Relaxed)
    }

    /// Jump target message ID as i64 (-1 if none). For backward compatibility.
    pub fn jump_target_message_id_i64(&self) -> i64 {
        self.jump_target_message_id().as_i64()
    }

    /// Set jump target (run loop only). Use MessageId::NONE to clear.
    pub fn set_jump_target_message_id(&self, message_id: MessageId) {
        self.jump_target_message_id
            .store(message_id, Ordering::Release);
    }

    /// Set jump target from i64 (-1 = none). For backward compatibility.
    pub fn set_jump_target_message_id_i64(&self, message_id: i64) {
        self.set_jump_target_message_id(MessageId::new(message_id));
    }

    pub fn set_me(&self, me: i64) {
        self.me.store(me, Ordering::Relaxed);
    }

    pub fn set_last_acknowledged_message_id(&self, message_id: i64) {
        self.last_acknowledged_message_id
            .store(message_id, Ordering::Relaxed);
    }

    // This is used to know if a message is being replied to.
    /// Set reply message (use MessageId::NONE to clear).
    pub fn set_reply_message(&self, message_id: MessageId, text: String) {
        self.reply_message_id.store(message_id, Ordering::Relaxed);
        *self.reply_message_text.lock().unwrap() = text;
    }

    pub fn clear_reply_message(&self) {
        self.set_reply_message(MessageId::NONE, String::new());
    }

    /// Set reply message from i64 (-1 = no reply). For backward compatibility.
    pub fn set_reply_message_i64(&self, message_id: i64, text: String) {
        self.set_reply_message(MessageId::new(message_id), text);
    }

    pub fn delete_message(&self, message_id: i64) {
        tracing::info!(message_id, "delete_message (local, user-initiated)");
        self.open_chat_messages().remove_message(message_id);
    }

    pub fn open_chat_user_status(&self) -> String {
        if let Some(user) = self.open_chat_user().as_ref() {
            return match &user.status {
                UserStatus::Empty => "Empty".to_string(),
                UserStatus::Online(_) => "Online".to_string(),
                UserStatus::Offline(offline) => {
                    format!("Last seen {}", format_timestamp(offline.was_online))
                }
                UserStatus::Recently(_) => "Last seen recently ".to_string(),
                UserStatus::LastWeek(_) => "Last seen LastWeek".to_string(),
                UserStatus::LastMonth(_) => "Last seen LastMonth ".to_string(),
            };
        }
        "".to_string()
    }

    pub fn unread_messages(&self) -> Vec<i64> {
        let mut unread_messages: Vec<i64> = Vec::new();
        let last_read_id = self.last_read_inbox_message_id();

        // Only include messages newer than the last read message
        for id in self.open_chat_messages().ordered_message_ids() {
            if id > last_read_id {
                unread_messages.push(id);
            }
        }
        unread_messages
    }

    pub fn last_read_inbox_message_id(&self) -> i64 {
        if let Some(topic) = self.open_topic() {
            return topic.last_read_inbox_message_id;
        }
        let opened_chat = self.chats().get(&self.open_chat_id().as_i64()).cloned();
        if let Some(opened_chat) = opened_chat {
            return opened_chat.last_read_inbox_message_id;
        }
        -1
    }

    pub fn last_read_outbox_message_id(&self) -> i64 {
        let opened_chat = self.chats().get(&self.open_chat_id().as_i64()).cloned();
        if let Some(opened_chat) = opened_chat {
            return opened_chat.last_read_outbox_message_id;
        }
        -1
    }

    pub fn try_name_from_chats_or_users(&self, user_id: i64) -> Option<String> {
        if self.name_from_chats(user_id).is_some() {
            return self.name_from_chats(user_id);
        }
        if let Some(user) = self.users().get(&user_id) {
            match user.usernames.as_ref() {
                Some(usernames) => {
                    if let Some(username) = usernames.active_usernames.first() {
                        return Some(username.clone());
                    }
                }
                None => {
                    return Some(user.first_name.clone());
                }
            }
        }
        None
    }

    pub fn name_from_chats(&self, chat_id: i64) -> Option<String> {
        if let Some(chat) = self.chats().get(&chat_id) {
            return Some(chat.title.clone());
        }
        None
    }

    /// Get reply message ID (MessageId::NONE if no reply).
    pub fn reply_message_id(&self) -> MessageId {
        self.reply_message_id.load(Ordering::Relaxed)
    }

    /// Get reply message ID as i64 (-1 if no reply). For backward compatibility.
    pub fn reply_message_id_i64(&self) -> i64 {
        self.reply_message_id().as_i64()
    }

    pub fn reply_message_text(&self) -> MutexGuard<'_, String> {
        self.reply_message_text.lock().unwrap()
    }

    /// What kind the open chat is, for its icon.
    pub fn open_chat_kind(&self) -> Option<ChatKind> {
        let chats = self.chats();
        let chat = chats.get(&self.open_chat_id().as_i64())?;
        let user = match &chat.r#type {
            ChatType::Private(p) => self.users().get(&p.user_id).cloned(),
            _ => None,
        };
        Some(self.kind_of(chat, user.as_ref()))
    }

    /// `user` is the other party of a private chat.
    fn kind_of(&self, chat: &Chat, user: Option<&User>) -> ChatKind {
        match &chat.r#type {
            ChatType::Private(p) if p.user_id == self.me() => ChatKind::Saved,
            ChatType::Private(_) if user.is_some_and(|u| matches!(u.r#type, UserType::Bot(_))) => {
                ChatKind::Bot
            }
            ChatType::Private(_) => ChatKind::Private,
            ChatType::Secret(_) => ChatKind::Secret,
            ChatType::Supergroup(group) if group.is_channel => ChatKind::Channel,
            ChatType::BasicGroup(_) | ChatType::Supergroup(_) => ChatKind::Group,
        }
    }

    /// Your own name, once TDLib has sent it.
    pub fn my_name(&self) -> Option<String> {
        let me = self.me();
        (me != 0)
            .then(|| self.try_name_from_chats_or_users(me))
            .flatten()
            .filter(|name| !name.is_empty())
    }

    /// A user's full name, or a chat's title.
    pub fn sender_name(&self, sender: TdMessageSender) -> Option<String> {
        match sender {
            TdMessageSender::User(user_id) => self.users().get(&user_id).map(|user| {
                format!("{} {}", user.first_name, user.last_name)
                    .trim()
                    .to_owned()
            }),
            TdMessageSender::Chat(chat_id) => self.name_from_chats(chat_id),
        }
    }

    /// Describes a new message for a desktop notification; the text is left
    /// out unless `show_preview`.
    pub fn message_alert(
        &self,
        message: &Message,
        show_preview: bool,
        is_silent: bool,
    ) -> MessageAlert {
        let entry = MessageEntry::from(message);
        let chat_title = self.name_from_chats(message.chat_id).unwrap_or_default();
        // Private and secret chats and channels are named after their only sender
        let sender_name = (entry.sender_id() != message.chat_id)
            .then(|| self.sender_name(entry.sender()))
            .flatten()
            .filter(|name| !name.is_empty() && *name != chat_title);
        MessageAlert {
            chat_id: message.chat_id,
            message_id: message.id,
            chat_title,
            sender_name,
            text: show_preview.then(|| entry.message_content_to_string()),
            is_silent,
        }
    }

    pub fn name_of_open_chat_id(&self) -> Option<String> {
        if let Some(chat) = self.chats().get(&self.open_chat_id().as_i64()) {
            return Some(chat.title.clone());
        }
        None
    }

    /// Whether a chat is a forum: a supergroup whose messages are split into topics.
    pub fn is_forum(&self, chat_id: i64) -> bool {
        let supergroup_id = match self.chats().get(&chat_id).map(|chat| &chat.r#type) {
            Some(ChatType::Supergroup(group)) if !group.is_channel => group.supergroup_id,
            _ => return false,
        };
        self.supergroups()
            .get(&supergroup_id)
            .is_some_and(|group| group.is_forum)
    }

    pub fn open_chat_is_forum(&self) -> bool {
        self.open_chat_is_forum.load(Ordering::Relaxed)
    }

    /// Set together with the open chat, see [`Self::is_forum`].
    pub fn set_open_chat_is_forum(&self, is_forum: bool) {
        self.open_chat_is_forum.store(is_forum, Ordering::Relaxed);
    }

    /// Topics of the open forum chat, in topic list order.
    pub fn open_chat_topics_snapshot(&self) -> Vec<TopicEntry> {
        self.open_chat_topics.lock().unwrap().clone()
    }

    /// Replaces the open chat's topics (backend / load task).
    pub fn set_open_chat_topics(&self, mut topics: Vec<TopicEntry>) {
        sort_topics(&mut topics);
        *self.open_chat_topics.lock().unwrap() = topics;
    }

    /// The open topic of a forum chat, if any.
    pub fn open_topic_id(&self) -> Option<i32> {
        Some(self.open_topic_id.load(Ordering::Relaxed)).filter(|id| *id != 0)
    }

    /// Opens a topic of the open chat (`None` closes it).
    pub fn set_open_topic_id(&self, topic_id: Option<i32>) {
        self.open_topic_id
            .store(topic_id.unwrap_or(0), Ordering::Relaxed);
    }

    pub fn open_topic(&self) -> Option<TopicEntry> {
        let topic_id = self.open_topic_id()?;
        self.open_chat_topics
            .lock()
            .unwrap()
            .iter()
            .find(|topic| topic.topic_id == topic_id)
            .cloned()
    }

    /// The open topic, to search or list its messages.
    pub fn open_message_topic(&self) -> Option<MessageTopic> {
        self.open_topic_id()
            .map(|forum_topic_id| MessageTopic::Forum(MessageTopicForum { forum_topic_id }))
    }

    /// The open topic, to send messages to: none for the General topic, where
    /// messages without a topic go.
    pub fn open_send_topic(&self) -> Option<MessageTopic> {
        self.open_topic()
            .filter(|topic| !topic.is_general)
            .and(self.open_message_topic())
    }

    /// Whether a message of the open chat in `topic` is shown: in a forum only
    /// the open topic's messages are.
    pub fn shows_message_topic(&self, topic: Option<&MessageTopic>) -> bool {
        match self.open_topic() {
            Some(open) => open.contains(topic),
            None => !self.open_chat_is_forum(),
        }
    }

    /// A message arrived in the open forum chat: it becomes its topic's last
    /// message and moves the topic up. False when no topic has it.
    pub fn add_open_chat_topic_message(
        &self,
        topic: Option<&MessageTopic>,
        message: &MessageEntry,
    ) -> bool {
        let unread = message.sender_id() != self.me() && !self.shows_message_topic(topic);
        let mut topics = self.open_chat_topics.lock().unwrap();
        let top = topics.iter().map(|t| t.order).max().unwrap_or(0);
        let Some(entry) = topics.iter_mut().find(|t| t.contains(topic)) else {
            return false;
        };
        entry.last_message = Some(message.clone());
        entry.order = entry.order.max(top + 1);
        if unread {
            entry.unread_count += 1;
        }
        sort_topics(&mut topics);
        true
    }

    /// A topic of the open chat was created or renamed, opened or closed.
    pub fn set_open_chat_topic_info(&self, info: &ForumTopicInfo) -> bool {
        if info.chat_id != self.open_chat_id().as_i64() {
            return false;
        }
        let mut topics = self.open_chat_topics.lock().unwrap();
        match topics
            .iter_mut()
            .find(|t| t.topic_id == info.forum_topic_id)
        {
            Some(entry) => entry.set_info(info),
            None => {
                let top = topics.iter().map(|t| t.order).max().unwrap_or(0);
                let mut entry = TopicEntry {
                    chat_id: info.chat_id,
                    topic_id: info.forum_topic_id,
                    order: top + 1,
                    ..TopicEntry::default()
                };
                entry.set_info(info);
                topics.push(entry);
                sort_topics(&mut topics);
            }
        }
        true
    }

    /// A topic of the open chat was read, pinned or muted.
    pub fn update_open_chat_topic(&self, update: &UpdateForumTopic) -> bool {
        if update.chat_id != self.open_chat_id().as_i64() {
            return false;
        }
        let mut topics = self.open_chat_topics.lock().unwrap();
        let Some(entry) = topics
            .iter_mut()
            .find(|t| t.topic_id == update.forum_topic_id)
        else {
            return false;
        };
        entry.apply(update);
        // The update has no unread count, but it is zero once the last message is read
        let last_id = entry.last_message.as_ref().map_or(0, MessageEntry::id);
        if update.last_read_inbox_message_id >= last_id {
            entry.unread_count = 0;
        }
        sort_topics(&mut topics);
        true
    }

    /// Build the main chat list for the UI. Lock `chats` before `chats_index`: the TDLib update
    /// task takes `chats` then `chats_index` when applying position updates; the previous
    /// opposite order here could deadlock after a large catch-up when the UI rebuilt the list
    /// concurrently with incoming updates.
    pub fn chat_list(&self) -> Vec<ChatListEntry> {
        let chats = self.chats();
        let chats_index = self.chats_index();
        self.entries_of(&chats, &chats_index)
    }

    /// The chats of the Archive chat list, like [`Self::chat_list`].
    pub fn archived_chat_list(&self) -> Vec<ChatListEntry> {
        let chats = self.chats();
        let archive_index = self.archive_index();
        self.entries_of(&chats, &archive_index)
    }

    /// The state of a chat that the chat actions toggle; None for an unknown chat.
    pub fn chat_state(&self, chat_id: i64) -> Option<ChatState> {
        let chats = self.chats();
        let chat = chats.get(&chat_id)?;
        let user = match &chat.r#type {
            ChatType::Private(p) => self.users().get(&p.user_id).cloned(),
            _ => None,
        };
        let position = |list: &ChatList| chat.positions.iter().find(|p| p.list == *list);
        let archived = position(&ChatList::Archive);
        let settings = &chat.notification_settings;
        Some(ChatState {
            chat_id,
            title: chat.title.clone(),
            kind: self.kind_of(chat, user.as_ref()),
            is_muted: !settings.use_default_mute_for && settings.mute_for > 0,
            is_archived: archived.is_some(),
            is_pinned: archived
                .or(position(&ChatList::Main))
                .is_some_and(|p| p.is_pinned),
            is_unread: chat.unread_count > 0 || chat.is_marked_as_unread,
            last_message_id: chat.last_message.as_ref().map_or(0, |m| m.id),
        })
    }

    /// The other party of a private or secret chat, if known.
    pub fn chat_user(&self, chat_id: i64) -> Option<User> {
        let user_id = match self.chats().get(&chat_id).map(|chat| &chat.r#type) {
            Some(ChatType::Private(p)) => p.user_id,
            Some(ChatType::Secret(s)) => s.user_id,
            _ => return None,
        };
        self.users().get(&user_id).cloned()
    }

    /// Whose full info the profile of `chat_id` shows; your own for your
    /// chat ID even before that chat is loaded. None for an unknown chat.
    pub fn profile_source(&self, chat_id: i64) -> Option<ProfileSource> {
        let me = self.me();
        let chats = self.chats();
        let Some(chat) = chats.get(&chat_id) else {
            return (me != 0 && chat_id == me).then_some(ProfileSource::User(me));
        };
        Some(match &chat.r#type {
            ChatType::Private(p) => ProfileSource::User(p.user_id),
            ChatType::Secret(s) => ProfileSource::User(s.user_id),
            ChatType::BasicGroup(g) => ProfileSource::BasicGroup(g.basic_group_id),
            ChatType::Supergroup(g) => ProfileSource::Supergroup(g.supergroup_id),
        })
    }

    /// The profile of a chat from what's cached; None for an unknown chat.
    /// The bio or description is there once the full info has been loaded.
    pub fn profile(&self, chat_id: i64) -> Option<Profile> {
        let source = self.profile_source(chat_id)?;
        let (title, kind) = {
            let chats = self.chats();
            match chats.get(&chat_id) {
                Some(chat) => {
                    let user = match source {
                        ProfileSource::User(user_id) => self.users().get(&user_id).cloned(),
                        _ => None,
                    };
                    (chat.title.clone(), self.kind_of(chat, user.as_ref()))
                }
                None => (String::new(), ChatKind::Saved),
            }
        };
        let mut profile = Profile {
            chat_id,
            kind,
            name: title,
            id: chat_id,
            ..Profile::default()
        };
        match source {
            ProfileSource::User(user_id) => {
                profile.id = user_id;
                profile.is_me = user_id == self.me();
                if let Some(user) = self.users().get(&user_id) {
                    let name = format!("{} {}", user.first_name, user.last_name);
                    if !name.trim().is_empty() {
                        profile.name = name.trim().to_owned();
                    }
                    profile.usernames = user
                        .usernames
                        .as_ref()
                        .map(|u| u.active_usernames.clone())
                        .unwrap_or_default();
                    profile.phone_number = user.phone_number.clone();
                    profile.status = match user.r#type {
                        UserType::Bot(_) => String::new(),
                        _ if profile.is_me => "online".to_owned(),
                        _ => user_status_text(&user.status),
                    };
                }
                if let Some(full) = self.users_full_info().get(&user_id) {
                    profile.is_loaded = true;
                    profile.bio = full
                        .bot_info
                        .as_ref()
                        .map(|bot| bot.description.clone())
                        .filter(|description| !description.is_empty())
                        .or_else(|| full.bio.as_ref().map(|bio| bio.text.clone()))
                        .unwrap_or_default();
                    profile.birthdate = full.birthdate.as_ref().map(birthdate_text);
                }
            }
            ProfileSource::BasicGroup(group_id) => {
                let members = self.basic_groups().get(&group_id).map(|g| g.member_count);
                if let Some(full) = self.basic_groups_full_info().get(&group_id) {
                    profile.is_loaded = true;
                    profile.bio = full.description.clone();
                }
                profile.status = members.map(member_count_text).unwrap_or_default();
            }
            ProfileSource::Supergroup(group_id) => {
                let mut members = 0;
                if let Some(group) = self.supergroups().get(&group_id) {
                    profile.usernames = group
                        .usernames
                        .as_ref()
                        .map(|u| u.active_usernames.clone())
                        .unwrap_or_default();
                    members = group.member_count;
                }
                if let Some(full) = self.supergroups_full_info().get(&group_id) {
                    profile.is_loaded = true;
                    profile.bio = full.description.clone();
                    members = members.max(full.member_count);
                }
                profile.status = member_count_text(members);
            }
        }
        Some(profile)
    }

    /// Whether a chat is in the Archive chat list.
    pub fn is_archived(&self, chat_id: i64) -> bool {
        self.archive_index()
            .iter()
            .any(|ord_chat| ord_chat.chat_id == chat_id)
    }

    /// The row standing for the archive at the top of the main chat list: the
    /// newest archived message, the archived chats' names as its preview and
    /// how many of them are unread. None while no chat is archived.
    pub fn archive_entry(&self) -> Option<ChatListEntry> {
        let archived = self.archived_chat_list();
        if archived.is_empty() {
            return None;
        }
        let unread: Vec<&ChatListEntry> = archived
            .iter()
            .filter(|chat| chat.unread_count > 0 || chat.is_marked_as_unread)
            .collect();
        let names: Vec<&str> = archived
            .iter()
            .map(|chat| chat.chat_name.as_str())
            .filter(|name| !name.is_empty())
            .take(ARCHIVE_PREVIEW_CHATS)
            .collect();
        Some(ChatListEntry {
            chat_name: ARCHIVE_NAME.to_owned(),
            kind: ChatKind::Archive,
            last_message: archived
                .iter()
                .filter_map(|chat| chat.last_message.as_ref())
                .max_by_key(|message| message.timestamp())
                .cloned(),
            summary: Some(names.join(", ")),
            unread_count: i32::try_from(unread.len()).unwrap_or(i32::MAX),
            unread_mention_count: archived.iter().map(|chat| chat.unread_mention_count).sum(),
            // Dimmed unless an unmuted chat is unread
            is_muted: unread.iter().all(|chat| chat.is_muted),
            ..ChatListEntry::default()
        })
    }

    /// The rows of the chats in `index`, in its order.
    fn entries_of(
        &self,
        chats: &HashMap<i64, Chat>,
        index: &BTreeSet<OrderedChat>,
    ) -> Vec<ChatListEntry> {
        let mut chat_list = Vec::with_capacity(index.len());
        for ord_chat in index.iter() {
            let mut entry = ChatListEntry {
                chat_id: ord_chat.chat_id,
                is_pinned: ord_chat.position.is_pinned,
                ..ChatListEntry::default()
            };
            if let Some(chat) = chats.get(&ord_chat.chat_id) {
                entry.is_marked_as_unread = chat.is_marked_as_unread;
                entry.chat_name = chat.title.clone();
                entry.last_read_inbox_message_id = Some(chat.last_read_inbox_message_id);
                entry.last_read_outbox_message_id = Some(chat.last_read_outbox_message_id);
                entry.unread_count = chat.unread_count;
                entry.last_message = chat.last_message.as_ref().map(MessageEntry::from);
                entry.unread_mention_count = chat.unread_mention_count;
                let settings = &chat.notification_settings;
                entry.is_muted = !settings.use_default_mute_for && settings.mute_for > 0;
                if let ChatType::Private(p) = &chat.r#type {
                    entry.user = self.users().get(&p.user_id).cloned();
                }
                entry.kind = self.kind_of(chat, entry.user.as_ref());
            }
            chat_list.push(entry);
        }
        chat_list
    }
}

impl Default for Store {
    fn default() -> Self {
        Self {
            users: Mutex::default(),
            basic_groups: Mutex::default(),
            supergroups: Mutex::default(),
            secret_chats: Mutex::default(),
            chats: Mutex::default(),
            chats_index: Mutex::default(),
            archive_index: Mutex::default(),
            users_full_info: Mutex::default(),
            basic_groups_full_info: Mutex::default(),
            supergroups_full_info: Mutex::default(),
            me: AtomicI64::new(0),
            open_chat_id: AtomicChatId::new(ChatId::NONE),
            open_chat_messages: Mutex::default(),
            open_chat_pinned: Mutex::default(),
            open_chat_user: Mutex::default(),
            open_chat_is_forum: AtomicBool::new(false),
            open_chat_topics: Mutex::default(),
            open_topic_id: AtomicI32::new(0),
            last_acknowledged_message_id: AtomicI64::new(0),
            from_message_id: AtomicI64::new(0),
            history_loading: AtomicBool::new(false),
            jump_target_message_id: AtomicMessageId::new(MessageId::NONE),
            reply_message_id: AtomicMessageId::new(MessageId::NONE),
            reply_message_text: Mutex::default(),
        }
    }
}

/// Pinned topics first, then the highest order, like TDLib's topic list.
fn sort_topics(topics: &mut [TopicEntry]) {
    topics.sort_by_key(|topic| {
        (
            std::cmp::Reverse(topic.is_pinned),
            std::cmp::Reverse(topic.order),
        )
    });
}
