use super::*;
use crate::ui::tests::create_test_app_context;
use crossterm::event::KeyModifiers;
use tokio::sync::mpsc::unbounded_channel;

fn create_test_prompt_window() -> PromptWindow {
    let app_context = create_test_app_context();
    PromptWindow::new(app_context)
}

#[test]
fn reply_message_action_sets_reply_mode() {
    let mut window = create_test_prompt_window();
    let (tx, _rx) = unbounded_channel();
    window.register_action_handler(tx).unwrap();
    window.update(Action::ReplyMessage(42, "hello".to_string()));
    assert!(matches!(window.current_mode(), Mode::Reply(42)));
}

#[test]
fn reply_title_handles_multibyte_preview_at_byte_cutoff() {
    let preview = "й".repeat(21);
    assert_eq!(
        PromptWindow::reply_title(&preview),
        format!("Replying to: {preview}")
    );

    let long_preview = "й".repeat(41);
    assert_eq!(
        PromptWindow::reply_title(&long_preview),
        format!("Replying to: {}...", "й".repeat(39))
    );
}

#[test]
fn edit_message_action_sets_edit_mode() {
    let mut window = create_test_prompt_window();
    let (tx, _rx) = unbounded_channel();
    window.register_action_handler(tx).unwrap();
    window.update(Action::EditMessage(10, "edit me".to_string()));
    assert!(matches!(window.current_mode(), Mode::Edit(10)));
}

#[test]
fn esc_in_reply_mode_cancels_to_normal() {
    let mut window = create_test_prompt_window();
    let (tx, mut rx) = unbounded_channel();
    window.register_action_handler(tx).unwrap();
    window.update(Action::ReplyMessage(1, "preview".to_string()));
    assert!(matches!(window.current_mode(), Mode::Reply(1)));
    let modifiers = Modifiers::from(KeyModifiers::empty());
    window.update(Action::Key(KeyCode::Esc, modifiers));
    assert!(matches!(window.current_mode(), Mode::Normal));
    // Should have sent HideChatWindowReply and FocusComponent(Chat)
    let _ = rx.try_recv();
    let _ = rx.try_recv();
}

#[test]
fn esc_in_edit_mode_cancels_to_normal() {
    let mut window = create_test_prompt_window();
    let (tx, mut rx) = unbounded_channel();
    window.register_action_handler(tx).unwrap();
    window.update(Action::EditMessage(2, "text".to_string()));
    assert!(matches!(window.current_mode(), Mode::Edit(2)));
    let modifiers = Modifiers::from(KeyModifiers::empty());
    window.update(Action::Key(KeyCode::Esc, modifiers));
    assert!(matches!(window.current_mode(), Mode::Normal));
    let _ = rx.try_recv();
    let _ = rx.try_recv();
}

#[test]
fn normal_mode_unchanged_after_esc() {
    let mut window = create_test_prompt_window();
    let (tx, _rx) = unbounded_channel();
    window.register_action_handler(tx).unwrap();
    assert!(matches!(window.current_mode(), Mode::Normal));
    let modifiers = Modifiers::from(KeyModifiers::empty());
    window.update(Action::Key(KeyCode::Esc, modifiers));
    assert!(matches!(window.current_mode(), Mode::Normal));
}

#[test]
fn wide_chars_wrap_by_display_width() {
    let mut window = create_test_prompt_window();
    let modifiers = Modifiers::from(KeyModifiers::empty());
    window.update_input(Rect::new(0, 0, 7, 3));

    window.update(Action::Key(KeyCode::Char('하'), modifiers.clone()));
    window.update(Action::Key(KeyCode::Char('니'), modifiers.clone()));
    window.update(Action::Key(KeyCode::Char('달'), modifiers.clone()));

    assert_eq!(window.input.text.len(), 2);
    assert_eq!(window.input.cursor, (1, 1));
    assert_eq!(
        window.input.text[0]
            .iter()
            .map(|cell| cell.c)
            .collect::<String>(),
        "하니"
    );
    assert_eq!(
        window.input.text[1]
            .iter()
            .map(|cell| cell.c)
            .collect::<String>(),
        "달"
    );
}

#[test]
fn letter_q_is_inserted_as_text() {
    let mut window = create_test_prompt_window();
    let modifiers = Modifiers::from(KeyModifiers::empty());
    window.update(Action::Key(KeyCode::Char('q'), modifiers));

    assert_eq!(window.input.text.len(), 1);
    assert_eq!(
        window.input.text[0]
            .iter()
            .map(|cell| cell.c)
            .collect::<String>(),
        "q"
    );
}
#[test]
fn enter_sends_the_current_text() {
    use crate::settings::ActionBinding;

    let app_context = create_test_app_context();
    let event = Event::Key(KeyCode::Enter, KeyModifiers::NONE);
    let send_action = match app_context
        .keymap_config()
        .get_map_of(Some(ComponentName::Prompt))
        .get(&event)
    {
        Some(ActionBinding::Single { action, .. }) => action.clone(),
        binding => panic!("expected enter prompt binding, got {binding:?}"),
    };
    assert_eq!(send_action, Action::PromptSendMessage);

    let mut window = PromptWindow::new(Arc::clone(&app_context));
    let (action_tx, _action_rx) = unbounded_channel();
    window.register_action_handler(action_tx).unwrap();
    let modifiers = Modifiers::from(KeyModifiers::empty());
    window.update(Action::Key(KeyCode::Char('h'), modifiers.clone()));
    window.update(Action::Key(KeyCode::Char('i'), modifiers));
    window.update(send_action);

    let mut rx = app_context.action_rx();
    let sent = std::iter::from_fn(|| rx.try_recv().ok())
        .find(|action| matches!(action, Action::SendMessage(..)));
    assert_eq!(sent, Some(Action::SendMessage("hi".into(), None)));
}

fn typed(window: &PromptWindow) -> String {
    window
        .input
        .text
        .iter()
        .map(|line| line.iter().map(|cell| cell.c).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn modified_enter_and_ctrl_j_insert_newlines() {
    use crate::app::event::Event;
    use crate::settings::ActionBinding;
    let mut window = create_test_prompt_window();
    let (tx, _rx) = unbounded_channel();
    window.register_action_handler(tx).unwrap();
    let keymap = window.app_context.keymap_config();
    for modifiers in [
        KeyModifiers::ALT,
        KeyModifiers::SHIFT,
        KeyModifiers::CONTROL,
    ] {
        // Not bound, so the key reaches the prompt
        let binding = keymap
            .get_map_of(Some(ComponentName::Prompt))
            .get(&Event::Key(KeyCode::Enter, modifiers));
        assert!(
            !matches!(binding, Some(ActionBinding::Single { .. })),
            "{modifiers:?}"
        );
    }
    let none = Modifiers::from(KeyModifiers::NONE);
    window.update(Action::Key(KeyCode::Char('a'), none.clone()));
    window.update(Action::Key(
        KeyCode::Enter,
        Modifiers::from(KeyModifiers::ALT),
    ));
    window.update(Action::Key(KeyCode::Char('b'), none.clone()));
    window.update(Action::Key(
        KeyCode::Char('j'),
        Modifiers::from(KeyModifiers::CONTROL),
    ));
    window.update(Action::Key(KeyCode::Char('c'), none));
    assert_eq!(typed(&window), "a\nb\nc");
}

#[test]
fn an_empty_prompt_sends_nothing() {
    let app_context = create_test_app_context();
    let mut window = PromptWindow::new(Arc::clone(&app_context));
    let (tx, _rx) = unbounded_channel();
    window.register_action_handler(tx).unwrap();
    window.update(Action::Key(
        KeyCode::Char(' '),
        Modifiers::from(KeyModifiers::NONE),
    ));
    window.update(Action::PromptSendMessage);
    let mut rx = app_context.action_rx();
    let sent: Vec<Action> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    assert!(
        !sent
            .iter()
            .any(|action| matches!(action, Action::SendMessage(..))),
        "{sent:?}"
    );
}

#[test]
fn a_reply_sent_elsewhere_ends_reply_mode() {
    let mut window = create_test_prompt_window();
    window.update(Action::ReplyMessage(42, "hello".to_string()));
    // A sticker answered the message
    window.update(Action::HideChatWindowReply);
    assert!(matches!(window.current_mode(), Mode::Normal));
}

mod drafts {
    use super::*;
    use libtgt::model::TopicEntry;
    use ratatui::{Terminal, backend::TestBackend};
    use tokio::sync::mpsc::UnboundedReceiver;

    /// A prompt in chat 5 with topics 3 and 4, drafts as given, topic 3 open.
    fn prompt_in_forum(draft_3: &str) -> (PromptWindow, UnboundedReceiver<Action>) {
        let app_context = create_test_app_context();
        let store = app_context.store();
        store.set_open_chat_id_i64(5);
        set_drafts(&app_context, draft_3, "");
        store.set_open_topic_id(Some(3));
        let mut window = PromptWindow::new(Arc::clone(&app_context));
        let (tx, rx) = unbounded_channel();
        window.register_action_handler(tx).unwrap();
        (window, rx)
    }

    /// The drafts of topics 3 and 4 as TDLib last sent them.
    fn set_drafts(app_context: &AppContext, draft_3: &str, draft_4: &str) {
        let topic = |topic_id, draft: &str| TopicEntry {
            chat_id: 5,
            topic_id,
            draft: draft.to_owned(),
            ..TopicEntry::default()
        };
        app_context
            .store()
            .set_open_chat_topics(vec![topic(3, draft_3), topic(4, draft_4)]);
    }

    fn draw(window: &mut PromptWindow) {
        let mut terminal = Terminal::new(TestBackend::new(40, 5)).unwrap();
        terminal
            .draw(|frame| window.draw(frame, frame.area()).unwrap())
            .unwrap();
    }

    fn type_text(window: &mut PromptWindow, text: &str) {
        for c in text.chars() {
            window.update(Action::Key(
                KeyCode::Char(c),
                Modifiers::from(KeyModifiers::empty()),
            ));
        }
    }

    fn saves(rx: &mut UnboundedReceiver<Action>) -> Vec<Action> {
        std::iter::from_fn(|| rx.try_recv().ok())
            .filter(|action| matches!(action, Action::SaveDraft(..)))
            .collect()
    }

    #[test]
    fn switching_topics_saves_one_draft_and_loads_the_other() {
        let (mut window, mut rx) = prompt_in_forum("hello");
        draw(&mut window);
        assert_eq!(typed(&window), "hello");
        type_text(&mut window, "!");
        assert_eq!(typed(&window), "hello!", "typed at the end of the draft");

        window.app_context.store().set_open_topic_id(Some(4));
        draw(&mut window);
        assert_eq!(
            saves(&mut rx),
            [Action::SaveDraft(5, Some(3), "hello!".into())]
        );
        assert_eq!(typed(&window), "");

        // Nothing written in topic 4: nothing to save
        window.app_context.store().set_open_topic_id(Some(3));
        draw(&mut window);
        assert!(saves(&mut rx).is_empty());
    }

    #[test]
    fn a_draft_changed_elsewhere_is_loaded_unless_edited_here() {
        let (mut window, mut rx) = prompt_in_forum("a");
        draw(&mut window);
        set_drafts(&window.app_context, "from phone", "");
        draw(&mut window);
        assert_eq!(typed(&window), "from phone");

        type_text(&mut window, "!");
        set_drafts(&window.app_context, "newer", "");
        draw(&mut window);
        assert_eq!(typed(&window), "from phone!", "what's typed here wins");
        assert!(saves(&mut rx).is_empty());
    }

    #[test]
    fn leaving_the_prompt_saves_an_edited_draft_once() {
        let (mut window, mut rx) = prompt_in_forum("");
        draw(&mut window);
        window.unfocus();
        assert!(saves(&mut rx).is_empty(), "nothing written");
        type_text(&mut window, "later");
        window.unfocus();
        window.unfocus();
        assert_eq!(
            saves(&mut rx),
            [Action::SaveDraft(5, Some(3), "later".into())]
        );
    }

    #[test]
    fn an_edited_message_is_not_a_draft_and_the_draft_comes_back() {
        let (mut window, mut rx) = prompt_in_forum("keep me");
        draw(&mut window);

        // Leaving an edit cancels it
        window.update(Action::EditMessage(1, "old text".into()));
        window.unfocus();
        assert!(matches!(window.current_mode(), Mode::Normal));
        assert_eq!(typed(&window), "keep me");

        // Esc cancels it too
        window.update(Action::EditMessage(1, "old text".into()));
        window.update(Action::Key(
            KeyCode::Esc,
            Modifiers::from(KeyModifiers::empty()),
        ));
        assert_eq!(typed(&window), "keep me");

        // Sending it brings the draft back
        window.update(Action::EditMessage(1, "old text".into()));
        type_text(&mut window, "!");
        window.update(Action::PromptSendMessage);
        assert_eq!(typed(&window), "keep me");
        assert!(saves(&mut rx).is_empty());
    }

    #[test]
    fn cancelling_a_reply_keeps_what_was_written() {
        let (mut window, _rx) = prompt_in_forum("");
        draw(&mut window);
        window.update(Action::ReplyMessage(1, "preview".into()));
        type_text(&mut window, "an answer");
        window.update(Action::Key(
            KeyCode::Esc,
            Modifiers::from(KeyModifiers::empty()),
        ));
        assert!(matches!(window.current_mode(), Mode::Normal));
        assert_eq!(typed(&window), "an answer");
    }
}

mod captions {
    use super::*;
    use crate::ui::tests::create_mock_media_message;

    fn prompt() -> (PromptWindow, tokio::sync::mpsc::UnboundedReceiver<Action>) {
        let mut window = create_test_prompt_window();
        let (tx, rx) = unbounded_channel();
        window.register_action_handler(tx).unwrap();
        (window, rx)
    }

    fn sent_files(rx: &mut tokio::sync::mpsc::UnboundedReceiver<Action>) -> Vec<Action> {
        std::iter::from_fn(|| rx.try_recv().ok())
            .filter(|action| matches!(action, Action::SendFile(..)))
            .collect()
    }

    fn type_text(window: &mut PromptWindow, text: &str) {
        for c in text.chars() {
            window.update(Action::Key(
                KeyCode::Char(c),
                Modifiers::from(KeyModifiers::empty()),
            ));
        }
    }

    #[test]
    fn a_picked_file_takes_the_text_as_its_caption() {
        let (mut window, mut rx) = prompt();
        type_text(&mut window, " holiday photos ");
        window.update(Action::UploadFile("/tmp/a.jpg".into()));
        assert_eq!(
            sent_files(&mut rx),
            [Action::SendFile(
                "/tmp/a.jpg".into(),
                "holiday photos".into(),
                None
            )]
        );
        assert_eq!(typed(&window), "");

        // Without text there's no caption
        window.update(Action::UploadFile("/tmp/b.zip".into()));
        assert_eq!(
            sent_files(&mut rx),
            [Action::SendFile("/tmp/b.zip".into(), String::new(), None)]
        );
    }

    #[test]
    fn a_file_picked_while_replying_is_the_reply() {
        let (mut window, mut rx) = prompt();
        window.update(Action::ReplyMessage(42, "question".into()));
        type_text(&mut window, "here");
        window.update(Action::UploadFile("/tmp/a.pdf".into()));
        assert_eq!(
            sent_files(&mut rx),
            [Action::SendFile(
                "/tmp/a.pdf".into(),
                "here".into(),
                Some(TdMessageReplyToMessage {
                    chat_id: 0,
                    message_id: 42
                })
            )]
        );
        assert!(matches!(window.current_mode(), Mode::Normal));
    }

    #[test]
    fn an_edit_in_progress_isn_t_a_caption() {
        let (mut window, mut rx) = prompt();
        window.update(Action::EditMessage(10, "fixed text".into()));
        window.update(Action::UploadFile("/tmp/a.pdf".into()));
        assert_eq!(
            sent_files(&mut rx),
            [Action::SendFile("/tmp/a.pdf".into(), String::new(), None)]
        );
        assert_eq!(typed(&window), "fixed text");
        assert!(matches!(window.current_mode(), Mode::Edit(10)));
    }

    #[test]
    fn a_caption_can_be_edited_away() {
        let (mut window, _rx) = prompt();
        // Sending goes through the app's channel
        let app_context = Arc::clone(&window.app_context);
        let mut rx = app_context.action_rx();
        window
            .app_context
            .store()
            .open_chat_messages()
            .insert_messages([create_mock_media_message(10, "a.zip")]);
        window.update(Action::EditMessage(10, String::new()));
        window.update(Action::PromptSendMessage);
        let edits: Vec<Action> = std::iter::from_fn(|| rx.try_recv().ok())
            .filter(|action| matches!(action, Action::SendMessageEdited(..)))
            .collect();
        assert_eq!(edits, [Action::SendMessageEdited(10, String::new())]);

        // A text message can't be emptied
        window.update(Action::EditMessage(11, String::new()));
        window.update(Action::PromptSendMessage);
        assert!(
            std::iter::from_fn(|| rx.try_recv().ok())
                .all(|action| !matches!(action, Action::SendMessageEdited(..)))
        );
    }
}

mod attachments {
    use super::*;
    use std::path::PathBuf;

    fn prompt() -> (PromptWindow, tokio::sync::mpsc::UnboundedReceiver<Action>) {
        let mut window = create_test_prompt_window();
        let (tx, rx) = unbounded_channel();
        window.register_action_handler(tx).unwrap();
        (window, rx)
    }

    fn sent_files(rx: &mut tokio::sync::mpsc::UnboundedReceiver<Action>) -> Vec<Action> {
        std::iter::from_fn(|| rx.try_recv().ok())
            .filter(|action| matches!(action, Action::SendFile(..)))
            .collect()
    }

    fn file(dir: &tempfile::TempDir, name: &str) -> PathBuf {
        let path = dir.path().join(name);
        std::fs::write(&path, b"x").unwrap();
        path
    }

    fn esc() -> Action {
        Action::Key(KeyCode::Esc, Modifiers::from(KeyModifiers::empty()))
    }

    #[test]
    fn dropped_files_are_attached_and_sent_with_the_caption() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (file(&dir, "a.jpg"), file(&dir, "b.pdf"));
        let (mut window, mut rx) = prompt();
        window.update(Action::Paste(format!("{} {}", a.display(), b.display())));
        assert_eq!(typed(&window), "", "not typed");
        assert_eq!(window.attachments, [a.clone(), b.clone()]);
        window.update(Action::Paste("look".into()));
        assert_eq!(typed(&window), "look");

        window.update(Action::PromptSendMessage);
        assert_eq!(
            sent_files(&mut rx),
            [
                Action::SendFile(a.display().to_string(), "look".into(), None),
                Action::SendFile(b.display().to_string(), String::new(), None),
            ]
        );
        assert!(window.attachments.is_empty());
        assert_eq!(typed(&window), "");
    }

    #[test]
    fn esc_turns_attached_files_back_into_text() {
        let dir = tempfile::tempdir().unwrap();
        let spaced = file(&dir, "my photo.png");
        let (mut window, mut rx) = prompt();
        window.update(Action::Paste(format!("'{}'", spaced.display())));
        assert_eq!(window.attachments.len(), 1);
        window.update(esc());
        assert!(window.attachments.is_empty());
        assert_eq!(typed(&window), format!("'{}'", spaced.display()));
        // Pasting that text again attaches the same file
        assert_eq!(
            crate::ui::attachment::dropped_files(&typed(&window)),
            Some(vec![spaced])
        );
        window.update(Action::PromptSendMessage);
        assert!(sent_files(&mut rx).is_empty());
    }

    #[test]
    fn nothing_is_attached_while_editing() {
        let dir = tempfile::tempdir().unwrap();
        let a = file(&dir, "a.jpg");
        let (mut window, _rx) = prompt();
        window.update(Action::EditMessage(10, String::new()));
        window.update(Action::Paste(a.display().to_string()));
        assert!(window.attachments.is_empty());
        assert_eq!(typed(&window), a.display().to_string());
    }

    #[test]
    fn the_title_names_the_attached_files() {
        let dir = tempfile::tempdir().unwrap();
        let a = file(&dir, "a.jpg");
        let (mut window, _rx) = prompt();
        window.focus();
        window.update(Action::Paste(a.display().to_string()));
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 4)).unwrap();
        terminal
            .draw(|frame| window.draw(frame, frame.area()).unwrap())
            .unwrap();
        let screen: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(screen.contains("a.jpg"), "{screen}");
        assert!(screen.contains("enter sends"), "{screen}");
    }

    #[cfg(not(feature = "media-preview"))]
    #[test]
    fn pasting_an_image_needs_media_preview() {
        let (mut window, mut rx) = prompt();
        window.update(Action::PromptPasteImage);
        assert!(window.attachments.is_empty());
        assert!(
            std::iter::from_fn(|| rx.try_recv().ok()).any(|action| matches!(
                action,
                Action::StatusMessage(ref m) if m.contains("media-preview")
            ))
        );
    }
}
