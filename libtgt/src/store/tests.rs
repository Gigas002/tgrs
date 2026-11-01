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

mod archive {
    use tdlib_rs::enums::{BlockList, ChatAvailableReactions, ChatList, ChatType};
    use tdlib_rs::types::{
        Chat, ChatAvailableReactionsAll, ChatNotificationSettings, ChatPosition, ChatTypePrivate,
    };

    use crate::model::{ChatKind, OrderedChat};
    use crate::store::{ARCHIVE_NAME, Store};

    pub(super) fn chat(id: i64, title: &str, unread_count: i32, muted: bool) -> Chat {
        Chat {
            id,
            r#type: ChatType::Private(ChatTypePrivate { user_id: id }),
            title: title.to_owned(),
            photo: None,
            accent_color_id: 0,
            background_custom_emoji_id: 0,
            upgraded_gift_colors: None,
            profile_accent_color_id: 0,
            profile_background_custom_emoji_id: 0,
            permissions: Default::default(),
            last_message: None,
            positions: Vec::new(),
            chat_lists: Vec::new(),
            message_sender_id: None,
            block_list: None,
            has_protected_content: false,
            is_translatable: false,
            is_marked_as_unread: false,
            view_as_topics: false,
            has_scheduled_messages: false,
            has_welcome_messages: false,
            can_be_deleted_only_for_self: false,
            can_be_deleted_for_all_users: false,
            can_be_reported: false,
            default_disable_notification: false,
            unread_count,
            last_read_inbox_message_id: 0,
            last_read_outbox_message_id: 0,
            unread_mention_count: 0,
            unread_reaction_count: 0,
            unread_poll_vote_count: 0,
            notification_settings: ChatNotificationSettings {
                use_default_mute_for: !muted,
                mute_for: if muted { i32::MAX } else { 0 },
                ..Default::default()
            },
            available_reactions: ChatAvailableReactions::All(ChatAvailableReactionsAll::default()),
            message_auto_delete_time: 0,
            emoji_status: None,
            background: None,
            theme: None,
            action_bar: None,
            business_bot_manage_bar: None,
            video_chat: Default::default(),
            pending_join_requests: None,
            reply_markup_message_id: 0,
            draft_message: None,
            client_data: String::new(),
        }
    }

    /// Puts `chat` in `list` at `order`.
    fn add(store: &Store, chat: Chat, list: ChatList, order: i64) {
        let ordered = OrderedChat {
            chat_id: chat.id,
            position: ChatPosition {
                list: list.clone(),
                order,
                is_pinned: false,
                source: None,
            },
        };
        store.chats().insert(chat.id, chat);
        match list {
            ChatList::Archive => store.archive_index().insert(ordered),
            _ => store.chats_index().insert(ordered),
        };
    }

    fn names(chats: &[crate::model::ChatListEntry]) -> Vec<&str> {
        chats.iter().map(|chat| chat.chat_name.as_str()).collect()
    }

    #[test]
    fn archived_chats_are_listed_apart_from_the_main_list() {
        let store = Store::default();
        add(&store, chat(1, "Friend", 0, false), ChatList::Main, 5);
        add(&store, chat(2, "Old group", 0, true), ChatList::Archive, 1);
        add(&store, chat(3, "Old bot", 0, true), ChatList::Archive, 2);

        assert_eq!(names(&store.chat_list()), ["Friend"]);
        assert_eq!(names(&store.archived_chat_list()), ["Old bot", "Old group"]);
        assert!(store.is_archived(2));
        assert!(!store.is_archived(1));
    }

    #[test]
    fn the_archive_row_counts_unread_chats_and_names_them() {
        let store = Store::default();
        assert!(store.archive_entry().is_none());

        add(&store, chat(2, "Old group", 4, true), ChatList::Archive, 1);
        add(&store, chat(3, "Old bot", 0, false), ChatList::Archive, 2);
        let entry = store.archive_entry().unwrap();
        assert_eq!(entry.kind, ChatKind::Archive);
        assert_eq!(entry.chat_name, ARCHIVE_NAME);
        assert_eq!(entry.summary.as_deref(), Some("Old bot, Old group"));
        assert_eq!(entry.unread_count, 1);
        // Only muted chats are unread
        assert!(entry.is_muted);

        add(&store, chat(4, "Loud", 1, false), ChatList::Archive, 3);
        let entry = store.archive_entry().unwrap();
        assert_eq!(entry.unread_count, 2);
        assert!(!entry.is_muted);
    }

    #[test]
    fn drafts_of_chats_and_topics_are_looked_up() {
        use crate::model::{TopicEntry, text_draft};
        let store = Store::default();
        assert_eq!(store.draft(1, None), "");

        let mut with_draft = chat(1, "Ada", 0, false);
        with_draft.draft_message = text_draft("half a thought", 1);
        add(&store, with_draft, ChatList::Main, 1);
        add(&store, chat(2, "Bob", 0, false), ChatList::Main, 2);
        assert_eq!(store.draft(1, None), "half a thought");
        assert_eq!(store.draft(2, None), "");
        let drafts: Vec<(i64, String)> = store
            .chat_list()
            .into_iter()
            .map(|entry| (entry.chat_id, entry.draft))
            .collect();
        assert_eq!(
            drafts,
            [(2, String::new()), (1, "half a thought".to_owned())]
        );

        // Topics are looked up among the open chat's
        store.set_open_chat_topics(vec![TopicEntry {
            chat_id: 1,
            topic_id: 3,
            draft: "for the topic".to_owned(),
            ..TopicEntry::default()
        }]);
        assert_eq!(store.draft(1, Some(3)), "for the topic");
        assert_eq!(store.draft(1, Some(4)), "");
        assert_eq!(store.draft(9, Some(3)), "");
    }

    #[test]
    fn pinned_chats_are_listed_top_first_per_chat_list() {
        let store = Store::default();
        let put = |chat_id: i64, list: ChatList, order: i64, is_pinned: bool| {
            let ordered = OrderedChat {
                chat_id,
                position: ChatPosition {
                    list: list.clone(),
                    order,
                    is_pinned,
                    source: None,
                },
            };
            match list {
                ChatList::Archive => store.archive_index().insert(ordered),
                _ => store.chats_index().insert(ordered),
            };
        };
        put(1, ChatList::Main, 50, true);
        put(2, ChatList::Main, 90, true);
        put(3, ChatList::Main, 99, false);
        put(4, ChatList::Archive, 10, true);
        put(5, ChatList::Archive, 20, false);
        assert_eq!(store.pinned_chat_ids(false), [2, 1]);
        assert_eq!(store.pinned_chat_ids(true), [4]);
    }

    #[test]
    fn chat_state_tells_what_chat_actions_toggle() {
        let store = Store::default();
        assert!(store.chat_state(2).is_none());

        let mut archived = chat(2, "Old group", 0, true);
        archived.is_marked_as_unread = true;
        archived.positions = vec![ChatPosition {
            list: ChatList::Archive,
            order: 1,
            is_pinned: true,
            source: None,
        }];
        store.chats().insert(2, archived);
        let state = store.chat_state(2).unwrap();
        assert_eq!(state.title, "Old group");
        assert!(state.is_muted && state.is_archived && state.is_pinned && state.is_unread);
        assert!(!state.is_left(), "a private chat is deleted");

        let mut main = chat(3, "Friend", 0, false);
        main.positions = vec![ChatPosition {
            list: ChatList::Main,
            order: 1,
            is_pinned: false,
            source: None,
        }];
        store.chats().insert(3, main);
        let state = store.chat_state(3).unwrap();
        assert!(!state.is_muted && !state.is_archived && !state.is_pinned && !state.is_unread);
        assert!(!state.is_blocked && state.is_blockable());

        store.chats().get_mut(&3).unwrap().block_list = Some(BlockList::Main);
        assert!(store.chat_state(3).unwrap().is_blocked);
        // Blocked from seeing your stories only isn't blocked
        store.chats().get_mut(&3).unwrap().block_list = Some(BlockList::Stories);
        assert!(!store.chat_state(3).unwrap().is_blocked);
    }
}

mod profile {
    use serde_json::json;
    use tdlib_rs::enums::{ChatType, UserStatus, UserType};
    use tdlib_rs::types::{
        ChatTypeSupergroup, Supergroup, SupergroupFullInfo, User, UserFullInfo, UserStatusRecently,
        UserTypeBot,
    };

    use super::archive::chat;
    use crate::model::{ChatKind, ProfileSource};
    use crate::store::Store;

    /// Deserializes a TDLib object from `value`, giving the top-level fields it
    /// leaves out a zero value, so fixtures name only what a test is about.
    fn from_json<T>(
        mut value: serde_json::Value,
        parse: impl Fn(serde_json::Value) -> Result<T, serde_json::Error>,
    ) -> T {
        const ZEROES: [&str; 6] = ["0", "false", "\"0\"", "\"\"", "[]", "{}"];
        let mut error = match parse(value.clone()) {
            Ok(object) => return object,
            Err(e) => e.to_string(),
        };
        loop {
            let field = error
                .strip_prefix("missing field `")
                .and_then(|rest| rest.split('`').next())
                .filter(|field| value.get(*field).is_none())
                .unwrap_or_else(|| panic!("bad fixture: {error}"))
                .to_owned();
            // The first zero of the field's type gets past it
            for zero in ZEROES {
                value[&field] = serde_json::from_str(zero).unwrap();
                match parse(value.clone()) {
                    Ok(object) => return object,
                    Err(e) if e.to_string().starts_with("invalid") => continue,
                    Err(e) => {
                        error = e.to_string();
                        break;
                    }
                }
            }
        }
    }

    fn user(id: i64, first_name: &str, username: &str, bot: bool) -> User {
        let mut user: User = from_json(
            json!({
            "id": id,
            "first_name": first_name,
            "last_name": "",
            "usernames": {
                "active_usernames": [username],
                "disabled_usernames": [],
                "editable_username": username,
                "collectible_usernames": [],
            },
            "phone_number": "4412345",
            "status": { "@type": "userStatusEmpty" },
            "type": { "@type": "userTypeRegular" },
            }),
            serde_json::from_value,
        );
        user.status = UserStatus::Recently(UserStatusRecently::default());
        if bot {
            user.r#type = UserType::Bot(UserTypeBot::default());
        }
        user
    }

    fn user_full_info(bio: &str) -> UserFullInfo {
        from_json(
            json!({
                "bio": { "text": bio, "entities": [] },
                "birthdate": { "day": 10, "month": 12, "year": 0 },
                "gift_settings": {
                    "show_gift_button": false,
                    "accepted_gift_types": {
                        "unlimited_gifts": false,
                        "limited_gifts": false,
                        "upgraded_gifts": false,
                        "gifts_from_channels": false,
                        "premium_subscription": false,
                    },
                },
            }),
            serde_json::from_value,
        )
    }

    fn supergroup(id: i64, is_channel: bool) -> Supergroup {
        from_json(
            json!({
            "id": id,
            "usernames": {
                "active_usernames": ["rustlang"],
                "disabled_usernames": [],
                "editable_username": "rustlang",
                "collectible_usernames": [],
            },
            "status": { "@type": "chatMemberStatusLeft" },
            "member_count": 40,
            "is_channel": is_channel,
            }),
            serde_json::from_value,
        )
    }

    fn supergroup_full_info(description: &str) -> SupergroupFullInfo {
        from_json(
            json!({ "description": description, "member_count": 42 }),
            serde_json::from_value,
        )
    }

    #[test]
    fn an_unknown_chat_has_no_profile_but_yours_does() {
        let store = Store::default();
        assert!(store.profile_source(9).is_none());
        assert!(store.profile(9).is_none());

        store.set_me(9);
        assert_eq!(store.profile_source(9), Some(ProfileSource::User(9)));
        store.users().insert(9, user(9, "Me", "myself", false));
        let profile = store.profile(9).unwrap();
        assert!(profile.is_me && !profile.is_loaded);
        assert_eq!(profile.name, "Me");
        assert_eq!(profile.status, "online");
    }

    #[test]
    fn a_person_s_profile_fills_in_once_the_full_info_is_loaded() {
        let store = Store::default();
        store.chats().insert(5, chat(5, "Ada", 0, false));
        store.users().insert(5, user(5, "Ada", "ada", false));
        let profile = store.profile(5).unwrap();
        assert_eq!(profile.kind, ChatKind::Private);
        assert_eq!(profile.usernames, ["ada"]);
        assert_eq!(profile.phone_number, "4412345");
        assert_eq!(profile.status, "recently");
        assert!(!profile.is_loaded && profile.bio.is_empty());

        store
            .users_full_info()
            .insert(5, user_full_info("Poet of science"));
        let profile = store.profile(5).unwrap();
        assert!(profile.is_loaded);
        assert_eq!(profile.bio, "Poet of science");
        assert_eq!(profile.birthdate.as_deref(), Some("10.12"));
    }

    #[test]
    fn a_bot_has_no_last_seen() {
        let store = Store::default();
        store.chats().insert(6, chat(6, "Helper", 0, false));
        store
            .users()
            .insert(6, user(6, "Helper", "helper_bot", true));
        let profile = store.profile(6).unwrap();
        assert_eq!(profile.kind, ChatKind::Bot);
        assert!(profile.status.is_empty());
    }

    #[test]
    fn a_channel_shows_its_description_and_subscribers() {
        let store = Store::default();
        let mut channel = chat(-100, "Rust News", 0, false);
        channel.r#type = ChatType::Supergroup(ChatTypeSupergroup {
            supergroup_id: 77,
            is_channel: true,
        });
        store.chats().insert(-100, channel);
        store.supergroups().insert(77, supergroup(77, true));
        assert_eq!(
            store.profile_source(-100),
            Some(ProfileSource::Supergroup(77))
        );
        let profile = store.profile(-100).unwrap();
        assert_eq!(profile.kind, ChatKind::Channel);
        assert_eq!(profile.name, "Rust News");
        assert_eq!(profile.usernames, ["rustlang"]);
        assert_eq!(profile.status, "40");
        assert_eq!(profile.id, -100);

        store
            .supergroups_full_info()
            .insert(77, supergroup_full_info("All about Rust"));
        let profile = store.profile(-100).unwrap();
        assert_eq!(profile.bio, "All about Rust");
        assert_eq!(profile.status, "42");
    }
}
