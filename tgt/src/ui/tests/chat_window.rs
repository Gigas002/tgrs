use super::*;
use crate::{
    app::action::{Action, MessageCommand, Modifiers},
    ui::tests::{create_mock_message, create_test_app_context},
};
use crossterm::event::{KeyCode, KeyModifiers};

fn create_test_chat_window() -> ChatWindow {
    let app_context = create_test_app_context();
    ChatWindow::new(app_context)
}

fn setup_messages(window: &mut ChatWindow, messages: Vec<MessageEntry>) {
    let tg_context = window.app_context.store();
    {
        let mut store = tg_context.open_chat_messages();
        store.clear();
        store.insert_messages(messages);
    }
    window.refresh_message_list_from_store();
}

#[test]
fn test_chat_window_selection_by_id() {
    let mut window = create_test_chat_window();
    let messages = vec![
        create_mock_message(100, "First"),
        create_mock_message(200, "Second"),
        create_mock_message(300, "Third"),
    ];
    setup_messages(&mut window, messages);
    window.message_list_state.select(Some(1));
    let selected_id = window
        .message_list_state
        .selected()
        .and_then(|idx| window.message_list.get(idx).map(|m| m.id()));
    assert_eq!(selected_id, Some(200));
}

#[test]
fn test_chat_window_navigation() {
    let mut window = create_test_chat_window();
    let messages = vec![
        create_mock_message(1, "One"),
        create_mock_message(2, "Two"),
        create_mock_message(3, "Three"),
    ];
    setup_messages(&mut window, messages);
    window.focus(); // Key events only handled when focused
    let modifiers = Modifiers::from(KeyModifiers::empty());
    window.update(Action::Key(KeyCode::Down, modifiers.clone()));
    window.update(Action::Key(KeyCode::Up, modifiers.clone()));
    let selected = window.message_list_state.selected();
    assert_eq!(selected, Some(0));
}

#[test]
fn test_chat_window_sort_actions_no_op() {
    let mut window = create_test_chat_window();
    window.update(Action::ChatWindowSortWithString("test".to_string()));
    window.update(Action::ChatWindowRestoreSort);
    // Chat message search is server-side only; these are no-ops
    assert_eq!(window.message_list.len(), 0);
}

fn with_action_rx(window: &mut ChatWindow) -> tokio::sync::mpsc::UnboundedReceiver<Action> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    window.register_action_handler(tx).unwrap();
    rx
}

fn sent(rx: &mut tokio::sync::mpsc::UnboundedReceiver<Action>) -> Vec<Action> {
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

/// Edit on someone else's message must be a no-op (does not send EditMessage, does not bug the chat).
/// create_mock_message uses sender user_id 1; test app context has me() = 0, so all mock messages are "others".
#[test]
fn test_edit_on_others_message_is_no_op() {
    let mut window = create_test_chat_window();
    let mut rx = with_action_rx(&mut window);
    let messages = vec![
        create_mock_message(100, "Other's message"),
        create_mock_message(200, "Another other"),
    ];
    setup_messages(&mut window, messages);
    window.message_list_state.select(Some(0));
    window.update(Action::SelectedMessage(MessageCommand::Edit));
    assert_eq!(window.message_list_state.selected(), Some(0));
    assert!(sent(&mut rx).is_empty());
}

#[test]
fn reply_focuses_the_prompt_with_the_message() {
    let mut window = create_test_chat_window();
    let mut rx = with_action_rx(&mut window);
    setup_messages(&mut window, vec![create_mock_message(100, "hi")]);
    window.update(Action::RunMessageCommand(MessageCommand::Reply, 100));
    assert_eq!(
        sent(&mut rx),
        vec![
            Action::FocusComponent(ComponentName::Prompt),
            Action::ReplyMessage(100, "hi".into()),
        ]
    );
}

#[test]
fn media_commands_need_media() {
    let mut window = create_test_chat_window();
    let mut rx = with_action_rx(&mut window);
    setup_messages(&mut window, vec![create_mock_message(100, "text only")]);
    for command in [
        MessageCommand::OpenMedia,
        MessageCommand::ViewMedia,
        MessageCommand::DownloadMedia,
    ] {
        window.update(Action::RunMessageCommand(command, 100));
        assert_eq!(
            sent(&mut rx),
            vec![Action::StatusMessage("This message has no media.".into())]
        );
    }
}

fn message_with_links(id: i64, links: &[&str]) -> MessageEntry {
    use libtgt::model::{TdMessageSender, TextSpan, TextStyle};
    let link = TextStyle {
        link: true,
        ..TextStyle::default()
    };
    let spans = links
        .iter()
        .map(|target| TextSpan::styled(*target, link))
        .collect();
    MessageEntry::formatted(id, TdMessageSender::User(2), vec![spans], 0)
}

#[test]
fn one_link_opens_and_several_are_picked() {
    use crate::app::action::PickerKind;
    let mut window = create_test_chat_window();
    let mut rx = with_action_rx(&mut window);
    setup_messages(
        &mut window,
        vec![
            message_with_links(1, &["a.example"]),
            message_with_links(2, &["a.example", "b.example"]),
        ],
    );
    window.update(Action::RunMessageCommand(MessageCommand::OpenLink, 1));
    assert_eq!(sent(&mut rx), vec![Action::OpenUrl("a.example".into())]);
    window.update(Action::RunMessageCommand(MessageCommand::OpenLink, 2));
    assert_eq!(
        sent(&mut rx),
        vec![Action::ShowPicker(PickerKind::Links(vec![
            "a.example".into(),
            "b.example".into()
        ]))]
    );
}

#[test]
fn react_pin_and_link_commands_go_to_telegram() {
    let mut window = create_test_chat_window();
    let mut rx = with_action_rx(&mut window);
    setup_messages(&mut window, vec![create_mock_message(5, "hi")]);
    for (command, expected) in [
        (MessageCommand::React, Action::LoadReactions(5)),
        (MessageCommand::TogglePin, Action::SetPinned(5, true)),
        (MessageCommand::CopyLink, Action::CopyMessageLink(5)),
    ] {
        window.update(Action::RunMessageCommand(command, 5));
        assert_eq!(sent(&mut rx), vec![expected], "{command:?}");
    }
    window.update(Action::RunMessageCommand(MessageCommand::GoToReply, 5));
    assert!(matches!(
        sent(&mut rx).as_slice(),
        [Action::StatusMessage(_)]
    ));
}

#[test]
fn message_actions_open_for_the_selected_message() {
    let mut window = create_test_chat_window();
    let mut rx = with_action_rx(&mut window);
    window.update(Action::ShowMessageActions);
    assert_eq!(
        sent(&mut rx),
        vec![Action::StatusMessage("Select a message first.".into())]
    );

    setup_messages(
        &mut window,
        vec![create_mock_message(1, "a"), create_mock_message(2, "b")],
    );
    window.message_list_state.select(Some(1));
    window.update(Action::ShowMessageActions);
    assert_eq!(sent(&mut rx), vec![Action::ShowMessageActionsFor(2)]);
}

#[cfg(feature = "media-preview")]
mod previews {
    use super::*;
    use crate::{
        app::action::{PreviewDecodedPayload, PreviewRequest, PreviewTarget},
        ui::tests::create_mock_photo_message,
    };
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, style::Color};

    const RED: Color = Color::Rgb(200, 10, 10);

    fn render(window: &mut ChatWindow) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(60, 30)).unwrap();
        terminal
            .draw(|frame| window.draw(frame, frame.area()).unwrap())
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn draw(window: &mut ChatWindow) -> String {
        let buffer = render(window);
        buffer
            .content()
            .chunks(usize::from(buffer.area.width))
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn photos_are_previewed_in_the_chat() {
        let mut window = create_test_chat_window();
        let mut rx = with_action_rx(&mut window);
        setup_messages(
            &mut window,
            vec![
                create_mock_message(1, "before"),
                create_mock_photo_message(2, 30),
            ],
        );

        let screen = draw(&mut window);
        assert!(screen.contains("[loading preview…]"), "{screen}");
        let request = PreviewRequest {
            target: PreviewTarget::Chat,
            message_id: 2,
            file_id: 30,
            path: None,
        };
        assert_eq!(sent(&mut rx), vec![Action::LoadPreview(request.clone())]);

        window.update(Action::PreviewDecoded(PreviewDecodedPayload {
            request,
            result: Ok(image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
                64,
                48,
                image::Rgb([200, 10, 10]),
            ))),
        }));
        let screen = draw(&mut window);
        assert!(!screen.contains("loading preview"), "{screen}");
        let buffer = render(&mut window);
        assert!(
            buffer
                .content()
                .iter()
                .any(|cell| cell.fg == RED || cell.bg == RED),
            "the image is drawn"
        );
        assert!(
            sent(&mut rx).is_empty(),
            "loaded previews aren't requested again"
        );
    }

    #[test]
    fn messages_without_images_get_no_preview() {
        let mut window = create_test_chat_window();
        let mut rx = with_action_rx(&mut window);
        setup_messages(
            &mut window,
            vec![
                create_mock_message(1, "text"),
                crate::ui::tests::create_mock_media_message(2, "a.zip"),
            ],
        );
        let screen = draw(&mut window);
        assert!(!screen.contains("preview"), "{screen}");
        assert!(sent(&mut rx).is_empty());
    }
}

mod layout {
    use super::*;
    use libtgt::model::TdMessageSender;
    use ratatui::{Terminal, backend::TestBackend};

    fn screen(window: &mut ChatWindow, width: u16, height: u16) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| window.draw(frame, frame.area()).unwrap())
            .unwrap();
        let buffer = terminal.backend().buffer();
        buffer
            .content()
            .chunks(usize::from(width))
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect()
    }

    fn message(id: i64, sender: i64, text: &str, timestamp: i32) -> MessageEntry {
        MessageEntry::plain(id, TdMessageSender::User(sender), text, timestamp)
    }

    // 2023-11-14 22:13 UTC
    const T: i32 = 1_700_000_000;

    #[test]
    fn messages_are_grouped_by_sender_with_day_separators() {
        let mut window = create_test_chat_window();
        setup_messages(
            &mut window,
            vec![
                message(1, 1, "first", T),
                message(2, 1, "second", T + 120),
                message(3, 2, "reply", T + 180),
                message(4, 2, "tomorrow", T + 86_400),
            ],
        );
        let rows = screen(&mut window, 50, 20);
        let text = rows.join("\n");
        // One header per sender group; the 2nd message has none
        assert_eq!(text.matches("first").count(), 1, "{text}");
        let first = rows.iter().position(|row| row.contains("first")).unwrap();
        let second = rows.iter().position(|row| row.contains("second")).unwrap();
        assert_eq!(
            second,
            first + 1,
            "grouped messages follow directly:\n{text}"
        );
        // Two day separators: before the first message and before "tomorrow"
        let separators = rows.iter().filter(|row| row.contains("── 20")).count();
        assert_eq!(separators, 2, "{text}");
        // Nothing is read yet: the unread divider comes before the first message
        let divider = rows.iter().position(|row| row.contains(" New ")).unwrap();
        assert!(divider < first, "{text}");
        assert_eq!(
            text.matches("Unknown").count(),
            3,
            "one header per group:\n{text}"
        );
    }

    #[test]
    fn reactions_and_the_pin_are_shown_under_the_message() {
        use libtgt::model::Reaction;
        let mut window = create_test_chat_window();
        let mut pinned = message(1, 1, "vote", T).with_reactions(vec![
            Reaction {
                emoji: "👍".into(),
                count: 3,
                chosen: true,
            },
            Reaction {
                emoji: "🔥".into(),
                count: 1,
                chosen: false,
            },
        ]);
        pinned.set_is_pinned(true);
        setup_messages(&mut window, vec![pinned]);
        let rows = screen(&mut window, 40, 12);
        let text = rows.join("\n");
        let vote = rows.iter().position(|row| row.contains("vote")).unwrap();
        // Wide emoji take two cells in the buffer, so compare without spaces
        let chips = rows[vote + 1].replace(' ', "");
        assert!(chips.contains("📌👍3🔥1"), "{text}");
    }

    #[test]
    fn the_selected_message_is_framed_by_a_card() {
        let mut window = create_test_chat_window();
        setup_messages(
            &mut window,
            vec![message(1, 1, "hello", T), message(2, 1, "world", T + 60)],
        );
        window.focus();
        window.message_list_state.select(Some(1));
        let rows = screen(&mut window, 40, 16);
        let text = rows.join("\n");
        // A grouped continuation gets an empty top border and its time below
        let world = rows.iter().position(|row| row.contains("world")).unwrap();
        assert!(rows[world - 1].contains('╭'), "{text}");
        assert!(rows[world].contains("│ world"), "{text}");
        assert!(rows[world + 1].contains('╰'), "{text}");
        assert!(
            rows[world + 1].contains(':'),
            "time on the bottom border:\n{text}"
        );
        assert!(!rows[world - 2].contains('╭'), "{text}");
    }
}
