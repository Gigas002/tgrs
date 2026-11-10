use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use tdlib_rs::enums::{AuthorizationState, ChatList, NotificationType, Update};
use tdlib_rs::types::{Chat, ChatPosition};
use tokio::sync::mpsc::UnboundedSender;

use crate::model::{ActivityKind, MessageAlert, MessageEntry, OrderedChat, SignInAlert};
use crate::store::Store;

/// What changed in the [`Store`], for the UI to react to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notification {
    /// Chats were added, reordered or changed (rate limited).
    ChatListChanged,
    /// A new message arrived in the open chat (sent or received).
    OpenChatMessageAdded(i64),
    /// A message in the open chat changed in place, e.g. its reactions.
    OpenChatMessageChanged(i64),
    /// A message in the open chat was pinned or unpinned.
    OpenChatPinsChanged,
    /// A topic of the open forum chat changed, or a message arrived in one.
    OpenChatTopicsChanged,
    /// A message to notify the user about (see [`super::Requests::enable_notifications`]).
    NewMessage(MessageAlert),
    /// Someone logged in to your account and the session isn't confirmed yet.
    NewSignIn(SignInAlert),
    /// A followed download progressed or finished.
    TransferChanged(i32),
    /// Someone in the open chat started or stopped typing, recording, ...
    OpenChatActivityChanged,
    /// The number of unread messages in unmuted chats of the main chat list.
    UnreadCountChanged(i32),
}

/// Minimum gap between two [`Notification::ChatListChanged`] (~1 frame at 60fps).
const CHAT_LIST_INTERVAL: Duration = Duration::from_millis(16);

/// Coalesces bursts of chat list changes into at most one notification per interval.
struct Throttle {
    last: Instant,
    pending: bool,
}

impl Throttle {
    fn new() -> Self {
        Self {
            last: Instant::now(),
            pending: false,
        }
    }

    fn request(&mut self, notify: &UnboundedSender<Notification>) {
        self.pending = true;
        self.flush(notify);
    }

    fn flush(&mut self, notify: &UnboundedSender<Notification>) {
        if self.pending && self.last.elapsed() >= CHAT_LIST_INTERVAL {
            let _ = notify.send(Notification::ChatListChanged);
            self.last = Instant::now();
            self.pending = false;
        }
    }
}

/// Receives TDLib updates until `can_quit` is set, applying them to `store`.
pub(super) async fn run(
    store: Arc<Store>,
    auth_tx: UnboundedSender<AuthorizationState>,
    can_quit: Arc<AtomicBool>,
    notify: UnboundedSender<Notification>,
) {
    tracing::info!("Starting handling updates from TDLib");
    let mut throttle = Throttle::new();

    while !can_quit.load(Ordering::Acquire) {
        match tdlib_rs::receive() {
            Some((update, _client_id)) => {
                apply(&store, update, &auth_tx, &notify, &mut throttle).await;
                throttle.flush(&notify);
                // Let the runtime schedule other tasks (main loop, I/O) during TDLib bursts.
                tokio::task::yield_now().await;
            }
            None => {
                // No updates available, sleep briefly to avoid busy-wait loop
                tokio::time::sleep(Duration::from_millis(10)).await;
                throttle.flush(&notify);
            }
        }
    }
}

/// Puts the final `message` in place of the one with the temporary `old_id`.
fn replace_sent_message(
    store: &Store,
    old_id: i64,
    message: &tdlib_rs::types::Message,
    notify: &UnboundedSender<Notification>,
) {
    let entry = MessageEntry::from(message);
    if store.open_chat_messages().replace_message(old_id, entry) {
        let _ = notify.send(Notification::OpenChatMessageAdded(message.id));
    }
}

/// Updates a chat known to the store; updates for unknown chats are dropped.
fn with_chat(store: &Store, chat_id: i64, f: impl FnOnce(&mut Chat)) -> bool {
    match store.chats().get_mut(&chat_id) {
        Some(chat) => {
            f(chat);
            true
        }
        None => false,
    }
}

async fn apply(
    store: &Store,
    update: Update,
    auth_tx: &UnboundedSender<AuthorizationState>,
    notify: &UnboundedSender<Notification>,
    throttle: &mut Throttle,
) {
    let open_chat_id = store.open_chat_id().as_i64();
    match update {
        // The message shown while sending gets its final id: replace it, or
        // loading the history later shows the message twice
        Update::MessageSendSucceeded(update) => {
            tracing::info!("Message sent: {:?}", update.message.id);
            if update.message.chat_id == open_chat_id {
                replace_sent_message(store, update.old_message_id, &update.message, notify);
            }
        }
        Update::MessageSendFailed(update) => {
            tracing::warn!("Message not sent: {}", update.error.message);
            if update.message.chat_id == open_chat_id {
                replace_sent_message(store, update.old_message_id, &update.message, notify);
            }
        }
        Update::MessageSendAcknowledged(update) => {
            store.set_last_acknowledged_message_id(update.message_id);
        }
        Update::AuthorizationState(update) => {
            let _ = auth_tx.send(update.authorization_state);
        }
        Update::User(update) => {
            store.users().insert(update.user.id, update.user);
        }
        Update::File(update) => {
            if store.apply_file_update(&update.file) {
                let _ = notify.send(Notification::TransferChanged(update.file.id));
            }
        }
        Update::UnconfirmedSession(update) => {
            if let Some(alert) = update
                .session
                .as_ref()
                .and_then(SignInAlert::from_unconfirmed)
            {
                let _ = notify.send(Notification::NewSignIn(alert));
            }
        }
        Update::UserStatus(update) => {
            if let Some(user) = store.users().get_mut(&update.user_id) {
                user.status = update.status;
            }
        }
        Update::BasicGroup(update) => {
            store
                .basic_groups()
                .insert(update.basic_group.id, update.basic_group);
        }
        Update::Supergroup(update) => {
            store
                .supergroups()
                .insert(update.supergroup.id, update.supergroup);
        }
        Update::SecretChat(update) => {
            store
                .secret_chats()
                .insert(update.secret_chat.id, update.secret_chat);
        }
        Update::NewChat(update) => {
            let mut chat = update.chat;
            store.chats().insert(chat.id, chat.clone());
            let positions = std::mem::take(&mut chat.positions);
            set_chat_positions(store, &mut chat, positions);
            throttle.request(notify);
        }
        Update::ChatTitle(update) => {
            if with_chat(store, update.chat_id, |chat| chat.title = update.title) {
                throttle.request(notify);
            }
        }
        Update::ChatPhoto(update) => {
            with_chat(store, update.chat_id, |chat| chat.photo = update.photo);
        }
        Update::ChatPermissions(update) => {
            with_chat(store, update.chat_id, |chat| {
                chat.permissions = update.permissions
            });
        }
        Update::ChatLastMessage(update) => {
            // Also received when a message is edited
            let changed = with_chat(store, update.chat_id, |chat| {
                chat.last_message = update.last_message;
                set_chat_positions(store, chat, update.positions);
            });
            if changed {
                throttle.request(notify);
            }
        }
        Update::ChatPosition(update) if is_indexed(&update.position.list) => {
            let changed = with_chat(store, update.chat_id, |chat| {
                let mut positions: Vec<ChatPosition> = Vec::new();
                if update.position.order != 0 {
                    positions.push(update.position.clone());
                }
                positions.extend(
                    chat.positions
                        .iter()
                        .filter(|p| p.list != update.position.list)
                        .cloned(),
                );
                set_chat_positions(store, chat, positions);
            });
            if changed {
                throttle.request(notify);
            }
        }
        Update::ChatReadInbox(update) => {
            let changed = with_chat(store, update.chat_id, |chat| {
                chat.last_read_inbox_message_id = update.last_read_inbox_message_id;
                chat.unread_count = update.unread_count;
            });
            if changed {
                throttle.request(notify);
            }
        }
        Update::ChatReadOutbox(update) => {
            let changed = with_chat(store, update.chat_id, |chat| {
                chat.last_read_outbox_message_id = update.last_read_outbox_message_id;
            });
            if changed {
                throttle.request(notify);
            }
        }
        Update::ChatActionBar(update) => {
            with_chat(store, update.chat_id, |chat| {
                chat.action_bar = update.action_bar
            });
        }
        Update::ChatAvailableReactions(update) => {
            with_chat(store, update.chat_id, |chat| {
                chat.available_reactions = update.available_reactions
            });
        }
        Update::ChatUnreadMentionCount(update) => {
            with_chat(store, update.chat_id, |chat| {
                chat.unread_mention_count = update.unread_mention_count
            });
        }
        Update::MessageMentionRead(update) => {
            with_chat(store, update.chat_id, |chat| {
                chat.unread_mention_count = update.unread_mention_count
            });
        }
        Update::ChatReplyMarkup(update) => {
            with_chat(store, update.chat_id, |chat| {
                chat.reply_markup_message_id =
                    update.reply_markup_message.as_ref().map_or(0, |m| m.id);
            });
        }
        Update::ChatDraftMessage(update) => {
            with_chat(store, update.chat_id, |chat| {
                chat.draft_message = update.draft_message;
                set_chat_positions(store, chat, update.positions);
            });
        }
        Update::ChatMessageSender(update) => {
            with_chat(store, update.chat_id, |chat| {
                chat.message_sender_id = update.message_sender_id
            });
        }
        Update::ChatMessageAutoDeleteTime(update) => {
            with_chat(store, update.chat_id, |chat| {
                chat.message_auto_delete_time = update.message_auto_delete_time
            });
        }
        Update::ChatNotificationSettings(update) => {
            // Muted chats are dimmed in the chat list
            let changed = with_chat(store, update.chat_id, |chat| {
                chat.notification_settings = update.notification_settings
            });
            if changed {
                throttle.request(notify);
            }
        }
        Update::ChatPendingJoinRequests(update) => {
            with_chat(store, update.chat_id, |chat| {
                chat.pending_join_requests = update.pending_join_requests
            });
        }
        Update::ChatBackground(update) => {
            with_chat(store, update.chat_id, |chat| {
                chat.background = update.background
            });
        }
        Update::ChatTheme(update) => {
            with_chat(store, update.chat_id, |chat| chat.theme = update.theme);
        }
        Update::ChatUnreadReactionCount(update) => {
            with_chat(store, update.chat_id, |chat| {
                chat.unread_reaction_count = update.unread_reaction_count
            });
        }
        Update::ChatDefaultDisableNotification(update) => {
            with_chat(store, update.chat_id, |chat| {
                chat.default_disable_notification = update.default_disable_notification
            });
        }
        Update::ChatIsMarkedAsUnread(update) => {
            let changed = with_chat(store, update.chat_id, |chat| {
                chat.is_marked_as_unread = update.is_marked_as_unread;
            });
            if changed {
                throttle.request(notify);
            }
        }
        Update::ChatBlockList(update) => {
            with_chat(store, update.chat_id, |chat| {
                chat.block_list = update.block_list
            });
        }
        Update::ChatHasScheduledMessages(update) => {
            with_chat(store, update.chat_id, |chat| {
                chat.has_scheduled_messages = update.has_scheduled_messages
            });
        }
        Update::MessageUnreadReactions(update) => {
            with_chat(store, update.chat_id, |chat| {
                chat.unread_reaction_count = update.unread_reaction_count
            });
        }
        Update::UserFullInfo(update) => {
            store
                .users_full_info()
                .insert(update.user_id, update.user_full_info);
        }
        Update::BasicGroupFullInfo(update) => {
            store
                .basic_groups_full_info()
                .insert(update.basic_group_id, update.basic_group_full_info);
        }
        Update::SupergroupFullInfo(update) => {
            store
                .supergroups_full_info()
                .insert(update.supergroup_id, update.supergroup_full_info);
        }
        // Only the open chat's messages are cached; updates for other chats are ignored.
        Update::NewMessage(update) if update.message.chat_id == open_chat_id => {
            let message = update.message;
            let entry = MessageEntry::from(&message);
            let topic = message.topic_id.as_ref();
            if store.add_open_chat_topic_message(topic, &entry) {
                let _ = notify.send(Notification::OpenChatTopicsChanged);
            }
            // A forum shows only the open topic's messages
            if !store.shows_message_topic(topic) {
                return;
            }
            store
                .open_chat_messages()
                .insert_messages(std::iter::once(entry));
            let _ = notify.send(Notification::OpenChatMessageAdded(message.id));
            // Let the main loop process the message before blocking in receive() again.
            tokio::task::yield_now().await;
        }
        Update::MessageContent(update) if update.chat_id == open_chat_id => {
            if let Some(entry) = store
                .open_chat_messages()
                .get_message_mut(update.message_id)
            {
                entry.set_message_content(&update.new_content);
                entry.set_is_edited(true);
                let _ = notify.send(Notification::OpenChatMessageChanged(update.message_id));
            }
        }
        Update::MessageIsPinned(update) if update.chat_id == open_chat_id => {
            if let Some(entry) = store
                .open_chat_messages()
                .get_message_mut(update.message_id)
            {
                entry.set_is_pinned(update.is_pinned);
            }
            let _ = notify.send(Notification::OpenChatPinsChanged);
        }
        Update::MessageInteractionInfo(update) if update.chat_id == open_chat_id => {
            if let Some(entry) = store
                .open_chat_messages()
                .get_message_mut(update.message_id)
            {
                entry.set_interaction_info(update.interaction_info.as_ref());
                let _ = notify.send(Notification::OpenChatMessageChanged(update.message_id));
            }
        }
        Update::DeleteMessages(update) if update.chat_id == open_chat_id => {
            // from_cache: TDLib evicted messages from its local cache (it does so
            // after a while idle); they still exist on the server. Our own copies stay,
            // and nothing needs reloading: loading older history here grew the cache
            // past its limit on every eviction, until the newest messages (and the
            // reader's position) were dropped.
            if update.from_cache {
                tracing::debug!(
                    chat_id = update.chat_id,
                    count = update.message_ids.len(),
                    "DeleteMessages from TDLib's cache, keeping them"
                );
                return;
            }
            tracing::info!(
                chat_id = update.chat_id,
                count = update.message_ids.len(),
                "DeleteMessages: removing messages from open chat"
            );
            let mut messages = store.open_chat_messages();
            for id in &update.message_ids {
                messages.remove_message(*id);
            }
            drop(messages);
            // The chat's last message may have changed
            throttle.request(notify);
        }
        // Notifications shown before a restart (updateActiveNotifications) are
        // not repeated; edits of shown ones (updateNotification) are ignored.
        Update::NotificationGroup(update) => {
            for notification in update.added_notifications {
                if let NotificationType::NewMessage(new) = notification.r#type {
                    let alert =
                        store.message_alert(&new.message, new.show_preview, notification.is_silent);
                    let _ = notify.send(Notification::NewMessage(alert));
                }
            }
        }
        Update::ChatAction(update) => {
            let open = store.set_chat_activity(
                update.chat_id,
                (&update.sender_id).into(),
                update.topic_id,
                ActivityKind::of(&update.action),
                std::time::Instant::now(),
            );
            if open {
                let _ = notify.send(Notification::OpenChatActivityChanged);
            }
        }
        Update::UnreadMessageCount(update) if update.chat_list == ChatList::Main => {
            let _ = notify.send(Notification::UnreadCountChanged(
                update.unread_unmuted_count,
            ));
        }
        Update::ForumTopicInfo(update) => {
            let changed = store.set_open_chat_topic_info(&update.info);
            if changed {
                let _ = notify.send(Notification::OpenChatTopicsChanged);
            }
        }
        Update::ForumTopic(update) => {
            let changed = store.update_open_chat_topic(&update);
            if changed {
                let _ = notify.send(Notification::OpenChatTopicsChanged);
            }
        }
        _ => {}
    }
}

/// Whether a chat list is kept ordered in the store: the main one and the archive.
fn is_indexed(list: &ChatList) -> bool {
    matches!(list, ChatList::Main | ChatList::Archive)
}

/// Lock `chats` before calling (see [`Store::chat_list`]).
fn set_chat_positions(store: &Store, chat: &mut Chat, positions: Vec<ChatPosition>) {
    let mut chats_index = store.chats_index();
    let mut archive_index = store.archive_index();
    for position in &chat.positions {
        if let Some(index) = index_of(&position.list, &mut chats_index, &mut archive_index) {
            index.remove(&OrderedChat {
                position: position.clone(),
                chat_id: chat.id,
            });
        }
    }

    chat.positions = positions;

    for position in &chat.positions {
        if let Some(index) = index_of(&position.list, &mut chats_index, &mut archive_index) {
            index.insert(OrderedChat {
                position: position.clone(),
                chat_id: chat.id,
            });
        }
    }
}

/// The index of `list` among the store's, see [`is_indexed`].
fn index_of<'a>(
    list: &ChatList,
    chats_index: &'a mut BTreeSet<OrderedChat>,
    archive_index: &'a mut BTreeSet<OrderedChat>,
) -> Option<&'a mut BTreeSet<OrderedChat>> {
    match list {
        ChatList::Main => Some(chats_index),
        ChatList::Archive => Some(archive_index),
        ChatList::Folder(_) => None,
    }
}
