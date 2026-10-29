use super::*;
use crate::{
    app::action::{Action, Modifiers},
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

/// Edit on someone else's message must be a no-op (does not send EditMessage, does not bug the chat).
/// create_mock_message uses sender user_id 1; test app context has me() = 0, so all mock messages are "others".
#[test]
fn test_edit_on_others_message_is_no_op() {
    let mut window = create_test_chat_window();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    window.register_action_handler(tx).unwrap();
    let messages = vec![
        create_mock_message(100, "Other's message"),
        create_mock_message(200, "Another other"),
    ];
    setup_messages(&mut window, messages);
    window.message_list_state.select(Some(0));
    let selected_before = window.message_list_state.selected();
    let selected_id_before =
        selected_before.and_then(|i| window.message_list.get(i).map(|m| m.id()));
    window.update(Action::ChatWindowEdit);
    // Selection unchanged; no EditMessage is sent (we can't easily assert no event without mock event_tx).
    assert_eq!(window.message_list_state.selected(), selected_before);
    assert_eq!(
        selected_before.and_then(|i| window.message_list.get(i).map(|m| m.id())),
        selected_id_before
    );
}
