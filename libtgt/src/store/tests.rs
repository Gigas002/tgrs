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
