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
fn configured_prompt_send_action_sends_current_text() {
    use crate::settings::ActionBinding;

    let app_context = create_test_app_context();
    let event = Event::Key(KeyCode::Enter, KeyModifiers::ALT);
    let send_action = match app_context
        .keymap_config()
        .get_map_of(Some(ComponentName::Prompt))
        .get(&event)
    {
        Some(ActionBinding::Single { action, .. }) => action.clone(),
        binding => panic!("expected alt+enter prompt binding, got {binding:?}"),
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
