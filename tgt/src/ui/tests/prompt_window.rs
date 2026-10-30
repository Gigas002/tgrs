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
