mod ids {
    use std::sync::atomic::Ordering;

    use crate::model::ids::*;

    #[test]
    fn chat_id_none() {
        assert!(ChatId::NONE.is_none());
        assert!(!ChatId::NONE.is_some());
        assert_eq!(ChatId::NONE.as_i64(), 0);
    }

    #[test]
    fn chat_id_some() {
        let id = ChatId::new(123);
        assert!(!id.is_none());
        assert!(id.is_some());
        assert_eq!(id.as_i64(), 123);
    }

    #[test]
    fn message_id_none() {
        assert!(MessageId::NONE.is_none());
        assert!(!MessageId::NONE.is_some());
        assert_eq!(MessageId::NONE.as_i64(), -1);
    }

    #[test]
    fn message_id_some() {
        let id = MessageId::new(456);
        assert!(!id.is_none());
        assert!(id.is_some());
        assert_eq!(id.as_i64(), 456);
    }

    #[test]
    fn atomic_chat_id() {
        let atomic = AtomicChatId::new(ChatId::NONE);
        assert_eq!(atomic.load(Ordering::Relaxed), ChatId::NONE);

        atomic.store(ChatId::new(789), Ordering::Relaxed);
        assert_eq!(atomic.load(Ordering::Relaxed).as_i64(), 789);
    }

    #[test]
    fn atomic_message_id() {
        let atomic = AtomicMessageId::new(MessageId::NONE);
        assert_eq!(atomic.load(Ordering::Relaxed), MessageId::NONE);

        atomic.store(MessageId::new(999), Ordering::Relaxed);
        assert_eq!(atomic.load(Ordering::Relaxed).as_i64(), 999);
    }
}

mod message {
    use crate::model::message::*;
    use tdlib_rs::enums::{MessageContent, MessageSender, TextEntityType};
    use tdlib_rs::types::{FormattedText, Message, MessageSenderUser, MessageText, TextEntity};

    fn message_with_entities(text: &str, entities: Vec<TextEntity>) -> Message {
        Message {
            id: 1,
            sender_id: MessageSender::User(MessageSenderUser { user_id: 1 }),
            receiver_id: None,
            chat_id: 1,
            sending_state: None,
            scheduling_state: None,
            is_outgoing: false,
            is_pinned: false,
            is_from_offline: false,
            can_be_saved: false,
            has_timestamped_media: false,
            is_channel_post: false,
            is_paid_star_suggested_post: false,
            is_paid_gram_suggested_post: false,
            contains_unread_mention: false,
            contains_unread_poll_votes: false,
            date: 0,
            edit_date: 0,
            forward_info: None,
            import_info: None,
            interaction_info: None,
            unread_reactions: vec![],
            fact_check: None,
            suggested_post_info: None,
            reply_to: None,
            topic_id: None,
            self_destruct_type: None,
            self_destruct_in: 0.0,
            auto_delete_in: 0.0,
            via_bot_user_id: 0,
            guest_bot_caller_id: None,
            sender_business_bot_user_id: 0,
            sender_boost_count: 0,
            sender_tag: String::new(),
            paid_message_star_count: 0,
            author_signature: String::new(),
            media_album_id: 0,
            effect_id: 0,
            restriction_info: None,
            summary_language_code: String::new(),
            content: MessageContent::MessageText(MessageText {
                text: FormattedText {
                    text: text.to_string(),
                    entities,
                },
                link_preview: None,
                link_preview_options: None,
            }),
            ephemeral_content: None,
            reply_markup: None,
        }
    }

    /// No duplicate or overlapping spans: output must equal the original text.
    #[test]
    fn format_message_content_single_entity_no_duplicate() {
        let msg = message_with_entities(
            "hello",
            vec![TextEntity {
                offset: 0,
                length: 5,
                r#type: TextEntityType::Bold,
            }],
        );
        let entry = MessageEntry::from(&msg);
        assert_eq!(entry.message_content_to_string(), "hello");
    }

    /// Two adjacent entities: output must be exact text, no duplicate segments.
    #[test]
    fn format_message_content_two_adjacent_entities_no_duplicate() {
        let msg = message_with_entities(
            "ab",
            vec![
                TextEntity {
                    offset: 0,
                    length: 1,
                    r#type: TextEntityType::Bold,
                },
                TextEntity {
                    offset: 1,
                    length: 1,
                    r#type: TextEntityType::Italic,
                },
            ],
        );
        let entry = MessageEntry::from(&msg);
        // Adjacent entities stay on same line (no newline between them)
        assert_eq!(entry.message_content_to_string(), "ab");
    }

    /// Two non-overlapping entities: output is exact text, all on same line.
    #[test]
    fn format_message_content_two_non_overlapping_no_duplicate() {
        let msg = message_with_entities(
            "hello world",
            vec![
                TextEntity {
                    offset: 0,
                    length: 5,
                    r#type: TextEntityType::Bold,
                },
                TextEntity {
                    offset: 6,
                    length: 5,
                    r#type: TextEntityType::Italic,
                },
            ],
        );
        let entry = MessageEntry::from(&msg);
        // All spans on same line (no newlines in original text)
        assert_eq!(entry.message_content_to_string(), "hello world");
    }

    /// Empty entities: plain text.
    #[test]
    fn format_message_content_empty_entities() {
        let msg = message_with_entities("plain", vec![]);
        let entry = MessageEntry::from(&msg);
        assert_eq!(entry.message_content_to_string(), "plain");
    }

    /// Multiline plain text: newlines should be preserved.
    #[test]
    fn format_message_content_multiline_plain() {
        let msg = message_with_entities("line1\nline2\nline3", vec![]);
        let entry = MessageEntry::from(&msg);
        assert_eq!(entry.message_content_to_string(), "line1\nline2\nline3");
    }

    /// Multiline text with entities: newlines should be preserved.
    #[test]
    fn format_message_content_multiline_with_entities() {
        let msg = message_with_entities(
            "hello\nworld",
            vec![TextEntity {
                offset: 0,
                length: 5,
                r#type: TextEntityType::Bold,
            }],
        );
        let entry = MessageEntry::from(&msg);
        assert_eq!(entry.message_content_to_string(), "hello\nworld");
    }

    /// Multiline text with entity spanning newline.
    #[test]
    fn format_message_content_entity_spans_newline() {
        let msg = message_with_entities(
            "hello\nworld",
            vec![TextEntity {
                offset: 0,
                length: 11, // Spans the entire text including newline
                r#type: TextEntityType::Bold,
            }],
        );
        let entry = MessageEntry::from(&msg);
        assert_eq!(entry.message_content_to_string(), "hello\nworld");
    }

    /// Call messages should display with duration.
    #[test]
    fn format_message_content_call_with_duration() {
        use tdlib_rs::enums::CallDiscardReason;
        use tdlib_rs::types::MessageCall;

        let msg = Message {
            id: 1,
            sender_id: MessageSender::User(MessageSenderUser { user_id: 1 }),
            receiver_id: None,
            chat_id: 1,
            sending_state: None,
            scheduling_state: None,
            is_outgoing: false,
            is_pinned: false,
            is_from_offline: false,
            can_be_saved: false,
            has_timestamped_media: false,
            is_channel_post: false,
            is_paid_star_suggested_post: false,
            is_paid_gram_suggested_post: false,
            contains_unread_mention: false,
            contains_unread_poll_votes: false,
            date: 0,
            edit_date: 0,
            forward_info: None,
            import_info: None,
            interaction_info: None,
            unread_reactions: vec![],
            fact_check: None,
            suggested_post_info: None,
            reply_to: None,
            topic_id: None,
            self_destruct_type: None,
            self_destruct_in: 0.0,
            auto_delete_in: 0.0,
            via_bot_user_id: 0,
            guest_bot_caller_id: None,
            sender_business_bot_user_id: 0,
            sender_boost_count: 0,
            sender_tag: String::new(),
            paid_message_star_count: 0,
            author_signature: String::new(),
            media_album_id: 0,
            effect_id: 0,
            restriction_info: None,
            summary_language_code: String::new(),
            content: MessageContent::MessageCall(MessageCall {
                unique_id: 0,
                is_video: false,
                discard_reason: CallDiscardReason::Declined,
                duration: 123,
            }),
            ephemeral_content: None,
            reply_markup: None,
        };

        let entry = MessageEntry::from(&msg);
        assert_eq!(entry.message_content_to_string(), "📞 Call (123s)");
    }

    /// Video call messages should display correctly.
    #[test]
    fn format_message_content_video_call() {
        use tdlib_rs::enums::CallDiscardReason;
        use tdlib_rs::types::MessageCall;

        let msg = Message {
            id: 1,
            sender_id: MessageSender::User(MessageSenderUser { user_id: 1 }),
            receiver_id: None,
            chat_id: 1,
            sending_state: None,
            scheduling_state: None,
            is_outgoing: false,
            is_pinned: false,
            is_from_offline: false,
            can_be_saved: false,
            has_timestamped_media: false,
            is_channel_post: false,
            is_paid_star_suggested_post: false,
            is_paid_gram_suggested_post: false,
            contains_unread_mention: false,
            contains_unread_poll_votes: false,
            date: 0,
            edit_date: 0,
            forward_info: None,
            import_info: None,
            interaction_info: None,
            unread_reactions: vec![],
            fact_check: None,
            suggested_post_info: None,
            reply_to: None,
            topic_id: None,
            self_destruct_type: None,
            self_destruct_in: 0.0,
            auto_delete_in: 0.0,
            via_bot_user_id: 0,
            guest_bot_caller_id: None,
            sender_business_bot_user_id: 0,
            sender_boost_count: 0,
            sender_tag: String::new(),
            paid_message_star_count: 0,
            author_signature: String::new(),
            media_album_id: 0,
            effect_id: 0,
            restriction_info: None,
            summary_language_code: String::new(),
            content: MessageContent::MessageCall(MessageCall {
                unique_id: 0,
                is_video: true,
                discard_reason: CallDiscardReason::HungUp,
                duration: 0,
            }),
            ephemeral_content: None,
            reply_markup: None,
        };

        let entry = MessageEntry::from(&msg);
        assert_eq!(entry.message_content_to_string(), "📹 Video Call");
    }

    fn with_content(content: MessageContent) -> MessageEntry {
        let mut msg = message_with_entities("", vec![]);
        msg.content = content;
        MessageEntry::from(&msg)
    }

    fn td_file(id: i32, size: i64, path: &str, downloaded: bool) -> tdlib_rs::types::File {
        tdlib_rs::types::File {
            id,
            size,
            local: tdlib_rs::types::LocalFile {
                path: path.to_owned(),
                is_downloading_completed: downloaded,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn document_becomes_media_with_caption() {
        use tdlib_rs::types::{Document, MessageDocument};
        let entry = with_content(MessageContent::MessageDocument(MessageDocument {
            document: Document {
                file_name: "backup.tar.gz".into(),
                mime_type: "application/gzip".into(),
                minithumbnail: None,
                thumbnail: None,
                document: td_file(9, 3 * 1024 * 1024, "", false),
            },
            caption: FormattedText {
                text: "weekly".into(),
                entities: vec![],
            },
        }));
        let media = entry.media().expect("document is media");
        assert_eq!(media.kind, MediaKind::Document);
        assert_eq!(media.file.id, 9);
        assert_eq!(media.suggested_file_name(), "backup.tar.gz");
        assert_eq!(media.file.downloaded_path(), None);
        assert_eq!(
            entry.message_content_to_string(),
            "📄 backup.tar.gz (3.0 MiB)\nweekly"
        );
    }

    #[test]
    fn video_preview_is_its_still_thumbnail_only() {
        use tdlib_rs::enums::ThumbnailFormat;
        use tdlib_rs::types::{MessageVideo, Thumbnail, Video};
        let video = |format| {
            with_content(MessageContent::MessageVideo(MessageVideo {
                video: Video {
                    duration: 75,
                    width: 320,
                    height: 180,
                    file_name: String::new(),
                    mime_type: "video/mp4".into(),
                    has_stickers: false,
                    supports_streaming: true,
                    minithumbnail: None,
                    thumbnail: Some(Thumbnail {
                        format,
                        width: 320,
                        height: 180,
                        file: td_file(2, 10, "", false),
                    }),
                    video: td_file(1, 100, "", false),
                },
                alternative_videos: vec![],
                storyboards: vec![],
                cover: None,
                start_timestamp: 0,
                caption: FormattedText::default(),
                show_caption_above_media: false,
                has_spoiler: false,
                is_secret: false,
            }))
        };
        let entry = video(ThumbnailFormat::Jpeg);
        let media = entry.media().unwrap();
        assert_eq!(media.kind, MediaKind::Video);
        assert_eq!(media.preview.as_ref().map(|f| f.id), Some(2));
        assert_eq!(media.suggested_file_name(), "video_1.mp4");
        assert_eq!(entry.message_content_to_string(), "🎥 Video (1:15)");

        // Animated MPEG-4 thumbnails can't be shown as a still image
        assert_eq!(video(ThumbnailFormat::Mpeg4).media().unwrap().preview, None);
    }

    #[test]
    fn photo_previews_itself() {
        use tdlib_rs::types::{MessagePhoto, Photo, PhotoSize};
        let size = |id| PhotoSize {
            photo: td_file(id, 0, "", false),
            ..Default::default()
        };
        let entry = with_content(MessageContent::MessagePhoto(MessagePhoto {
            photo: Photo {
                sizes: vec![size(1), size(2)],
                ..Default::default()
            },
            video: None,
            caption: FormattedText::default(),
            show_caption_above_media: false,
            has_spoiler: false,
            is_secret: false,
        }));
        let media = entry.media().unwrap();
        // The largest size is the last one
        assert_eq!(media.file.id, 2);
        assert_eq!(media.preview, Some(media.file.clone()));
        assert_eq!(media.suggested_file_name(), "photo_2.jpg");
        assert_eq!(entry.pinned_strip_preview(50), "📷 Photo");
    }

    #[test]
    fn downloaded_path_requires_a_complete_file_on_disk() {
        let dir = std::env::temp_dir();
        let file = dir.join(format!("libtgt-media-test-{}", std::process::id()));
        std::fs::write(&file, "x").unwrap();
        let path = file.to_str().unwrap();

        assert_eq!(
            MediaFile::from(&td_file(1, 1, path, true)).downloaded_path(),
            Some(path)
        );
        assert_eq!(
            MediaFile::from(&td_file(1, 1, path, false)).downloaded_path(),
            None
        );
        std::fs::remove_file(&file).unwrap();
        assert_eq!(
            MediaFile::from(&td_file(1, 1, path, true)).downloaded_path(),
            None
        );
        // The original name of a downloaded file without one is its cache name
        let media = Media {
            kind: MediaKind::Voice,
            file: MediaFile::from(&td_file(4, 1, "/cache/voice/file_4.oga", true)),
            file_name: String::new(),
            mime_type: "audio/ogg".into(),
            duration_secs: 3,
            preview: None,
        };
        assert_eq!(media.suggested_file_name(), "file_4.oga");
    }

    #[test]
    fn sizes_and_durations_are_human_readable() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(1536), "1.5 KiB");
        assert_eq!(format_size(5 * 1024 * 1024 * 1024), "5.0 GiB");
        assert_eq!(format_duration(5), "0:05");
        assert_eq!(format_duration(75), "1:15");
        assert_eq!(format_duration(3725), "1:02:05");
        assert_eq!(format_duration(-1), "0:00");
    }
}
