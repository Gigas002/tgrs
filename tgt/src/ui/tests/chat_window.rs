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
