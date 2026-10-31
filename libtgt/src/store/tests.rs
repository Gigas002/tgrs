mod open_chat {
    use crate::model::MessageEntry;
    use crate::store::open_chat::*;

    fn make_entry(id: i64) -> MessageEntry {
        MessageEntry::plain(id, crate::model::td_enums::TdMessageSender::User(0), "", 0)
    }

    #[test]
    fn empty_store() {
        let store = OpenChatMessageStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
        assert!(store.ordered_message_ids().is_empty());
        assert!(store.get_message(1).is_none());
        assert!(store.oldest_message_id().is_none());
        assert!(store.newest_message_id().is_none());
        assert_eq!(store.from_message_id_for_load_older(), 0);
    }

    #[test]
    fn insert_and_get() {
        let mut store = OpenChatMessageStore::new();
        let e1 = make_entry(10);
        let e2 = make_entry(20);
        store.insert_messages([e1.clone(), e2.clone()]);
        assert_eq!(store.len(), 2);
        assert_eq!(store.get_message(10).map(|e| e.id()), Some(10));
        assert_eq!(store.get_message(20).map(|e| e.id()), Some(20));
        assert!(store.get_message(15).is_none());
    }

    #[test]
    fn ordered_ids_ascending() {
        let mut store = OpenChatMessageStore::new();
        store.insert_messages([make_entry(30), make_entry(10), make_entry(20)]);
        assert_eq!(store.ordered_message_ids(), [10, 20, 30]);
    }

    #[test]
    fn view_window_oldest_newest() {
        let mut store = OpenChatMessageStore::new();
        assert!(store.oldest_message_id().is_none());
        assert!(store.newest_message_id().is_none());
        store.insert_messages([make_entry(100), make_entry(50)]);
        assert_eq!(store.oldest_message_id(), Some(50));
        assert_eq!(store.newest_message_id(), Some(100));
        store.insert_messages([make_entry(25)]);
        assert_eq!(store.oldest_message_id(), Some(25));
        assert_eq!(store.newest_message_id(), Some(100));
    }

    #[test]
    fn clear_resets_all() {
        let mut store = OpenChatMessageStore::new();
        store.insert_messages([make_entry(1), make_entry(2)]);
        store.clear();
        assert!(store.is_empty());
        assert!(store.ordered_message_ids().is_empty());
        assert!(store.get_message(1).is_none());
        assert!(store.oldest_message_id().is_none());
        assert!(store.newest_message_id().is_none());
    }

    #[test]
    fn from_message_id_for_load_older() {
        let mut store = OpenChatMessageStore::new();
        assert_eq!(store.from_message_id_for_load_older(), 0);
        store.insert_messages([make_entry(100), make_entry(50)]);
        assert_eq!(store.from_message_id_for_load_older(), 50);
    }

    #[test]
    fn cache_evicts_oldest_when_exceeding_max() {
        let mut store = OpenChatMessageStore::new();

        // Insert MAX_MESSAGES_IN_CACHE + 10 messages
        let extra = 10;
        let total = MAX_MESSAGES_IN_CACHE + extra;
        let messages: Vec<_> = (1..=total as i64).map(make_entry).collect();
        store.insert_messages(messages);

        // Should evict down to MIN_MESSAGES_AFTER_EVICTION
        assert_eq!(store.len(), MIN_MESSAGES_AFTER_EVICTION);

        // Some oldest messages should be evicted
        assert!(store.get_message(1).is_none());

        // Newest messages should still be present
        assert!(store.get_message(total as i64).is_some());
        assert!(store.get_message((total - 50) as i64).is_some());
    }

    #[test]
    fn cache_eviction_on_new_message() {
        let mut store = OpenChatMessageStore::new();

        // Fill cache to MAX_MESSAGES_IN_CACHE (IDs 1..2000)
        store.insert_messages((1..=MAX_MESSAGES_IN_CACHE as i64).map(make_entry));
        assert_eq!(store.len(), MAX_MESSAGES_IN_CACHE);

        // Add one more message (triggers eviction)
        store.insert_messages([make_entry((MAX_MESSAGES_IN_CACHE + 1) as i64)]);

        // Should evict down to MIN_MESSAGES_AFTER_EVICTION
        assert_eq!(store.len(), MIN_MESSAGES_AFTER_EVICTION);

        // Oldest message (ID=1) should be evicted
        assert!(store.get_message(1).is_none());

        // Newest message should be present
        assert!(
            store
                .get_message((MAX_MESSAGES_IN_CACHE + 1) as i64)
                .is_some()
        );
    }
}

mod topics {
    use tdlib_rs::enums::{MessageSender, MessageTopic};
    use tdlib_rs::types::{ForumTopicInfo, MessageSenderUser, MessageTopicForum, UpdateForumTopic};

    use crate::model::{MessageEntry, TdMessageSender, TopicEntry};
    use crate::store::Store;

    const CHAT: i64 = -100;

    fn topic(topic_id: i32, name: &str, order: i64) -> TopicEntry {
        TopicEntry {
            chat_id: CHAT,
            topic_id,
            name: name.to_owned(),
            order,
            is_general: topic_id == 1,
            ..TopicEntry::default()
        }
    }

    fn in_topic(topic_id: i32) -> MessageTopic {
        MessageTopic::Forum(MessageTopicForum {
            forum_topic_id: topic_id,
        })
    }

    /// A forum chat with General (1), Rust (2) and Go (3) open, nothing selected.
    fn forum() -> Store {
        let store = Store::default();
        store.set_open_chat_id_i64(CHAT);
        store.set_open_chat_is_forum(true);
        store.set_open_chat_topics(vec![
            topic(1, "General", 10),
            topic(3, "Go", 30),
            topic(2, "Rust", 20),
        ]);
        store
    }

    fn names(store: &Store) -> Vec<String> {
        store
            .open_chat_topics_snapshot()
            .into_iter()
            .map(|t| t.name)
            .collect()
    }

    #[test]
    fn topics_are_sorted_by_order() {
        assert_eq!(names(&forum()), ["Go", "Rust", "General"]);
    }

    #[test]
    fn only_the_open_topic_is_shown() {
        let store = Store::default();
        // Not a forum: everything is
        assert!(store.shows_message_topic(None));

        let store = forum();
        assert!(!store.shows_message_topic(Some(&in_topic(2))));
        store.set_open_topic_id(Some(2));
        assert!(store.shows_message_topic(Some(&in_topic(2))));
        assert!(!store.shows_message_topic(Some(&in_topic(3))));
        assert!(!store.shows_message_topic(None));
        // Messages without a topic are in General
        store.set_open_topic_id(Some(1));
        assert!(store.shows_message_topic(None));
    }

    #[test]
    fn messages_to_general_are_sent_without_a_topic() {
        let store = forum();
        store.set_open_topic_id(Some(1));
        assert_eq!(store.open_send_topic(), None);
        assert_eq!(store.open_message_topic(), Some(in_topic(1)));
        store.set_open_topic_id(Some(2));
        assert_eq!(store.open_send_topic(), Some(in_topic(2)));
    }

    #[test]
    fn a_new_message_moves_its_topic_up_and_counts_as_unread() {
        let store = forum();
        store.set_me(1);
        store.set_open_topic_id(Some(3));
        let message = MessageEntry::plain(5, TdMessageSender::User(2), "hi", 0);

        assert!(store.add_open_chat_topic_message(None, &message));
        assert_eq!(names(&store), ["General", "Go", "Rust"]);
        let general = &store.open_chat_topics_snapshot()[0];
        assert_eq!(general.unread_count, 1);
        assert_eq!(general.last_message.as_ref().map(MessageEntry::id), Some(5));

        // Not in the open topic's unread count, nor in ours
        let mine = MessageEntry::plain(6, TdMessageSender::User(1), "me", 0);
        assert!(store.add_open_chat_topic_message(Some(&in_topic(2)), &mine));
        let seen = MessageEntry::plain(7, TdMessageSender::User(2), "seen", 0);
        assert!(store.add_open_chat_topic_message(Some(&in_topic(3)), &seen));
        let topics = store.open_chat_topics_snapshot();
        assert!(
            topics
                .iter()
                .filter(|t| !t.is_general)
                .all(|t| t.unread_count == 0)
        );

        assert!(!store.add_open_chat_topic_message(Some(&in_topic(9)), &seen));
    }

    #[test]
    fn reading_a_topic_clears_its_unread_count() {
        let store = forum();
        let message = MessageEntry::plain(5, TdMessageSender::User(2), "hi", 0);
        store.add_open_chat_topic_message(Some(&in_topic(2)), &message);
        let update = UpdateForumTopic {
            chat_id: CHAT,
            forum_topic_id: 2,
            is_pinned: true,
            last_read_inbox_message_id: 5,
            last_read_outbox_message_id: 0,
            unread_mention_count: 0,
            unread_reaction_count: 0,
            unread_poll_vote_count: 0,
            notification_settings: Default::default(),
            draft_message: None,
        };

        assert!(store.update_open_chat_topic(&update));
        let rust = &store.open_chat_topics_snapshot()[0];
        assert_eq!((rust.name.as_str(), rust.unread_count), ("Rust", 0));
        assert!(rust.is_pinned);
        assert!(!store.update_open_chat_topic(&UpdateForumTopic {
            chat_id: 7,
            ..update
        }));
    }

    #[test]
    fn topic_info_renames_or_adds_a_topic() {
        let store = forum();
        let info = |forum_topic_id, name: &str| ForumTopicInfo {
            chat_id: CHAT,
            forum_topic_id,
            name: name.to_owned(),
            icon: Default::default(),
            creation_date: 0,
            creator_id: MessageSender::User(MessageSenderUser { user_id: 1 }),
            is_general: false,
            is_outgoing: false,
            is_closed: false,
            is_hidden: false,
            is_name_implicit: false,
        };

        assert!(store.set_open_chat_topic_info(&info(2, "Rust 🦀")));
        assert!(store.set_open_chat_topic_info(&info(4, "Zig")));
        assert_eq!(names(&store), ["Zig", "Go", "Rust 🦀", "General"]);
        assert!(!store.set_open_chat_topic_info(&ForumTopicInfo {
            chat_id: 7,
            ..info(5, "elsewhere")
        }));
    }
}
