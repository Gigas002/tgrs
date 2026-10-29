use super::*;
use crate::{
    app::action::{Action, Modifiers},
    ui::tests::{create_mock_chat, create_mock_message, create_test_app_context},
};
use crossterm::event::{KeyCode, KeyModifiers};

fn create_test_chat_list_window() -> ChatListWindow {
    let app_context = create_test_app_context();
    ChatListWindow::new(app_context)
}

#[test]
fn test_chat_list_entry_without_last_message_has_two_lines() {
    let app_context = create_test_app_context();
    // No last_message.
    let entry = create_mock_chat(1, "Zöli");

    let text = chat_list_item_text(&entry, &app_context);

    let line_count = text.iter().count();
    assert_eq!(
        line_count, 2,
        "Chat list entry without last message must have exactly 2 lines (name + [No message]) for consistent height and click mapping; got {}",
        line_count
    );
}

#[test]
fn test_chat_list_entry_with_empty_last_message_has_two_lines() {
    let app_context = create_test_app_context();
    let mut entry = create_mock_chat(1, "Pandúr Katinka");
    // last_message present but with empty content (e.g. "joined Telegram" → backend sends empty message).
    entry.last_message = Some(create_mock_message(1, ""));

    let text = chat_list_item_text(&entry, &app_context);

    let line_count = text.iter().count();
    assert_eq!(
        line_count, 2,
        "Chat list entry with empty last message (e.g. joined Telegram) must have exactly 2 lines (name + [No message]); got {}",
        line_count
    );
}

fn setup_chats(window: &mut ChatListWindow, chat_names: &[&str]) {
    let mut chats = Vec::new();
    for (i, name) in chat_names.iter().enumerate() {
        chats.push(create_mock_chat(i as i64 + 1, name));
    }
    // Directly set the visible_chats (using internal access for testing)
    window.visible_chats = chats;
}

#[test]
fn test_chat_list_search_initial_state() {
    let window = create_test_chat_list_window();
    assert!(!window.search_mode, "Search mode should be false initially");
}

#[test]
fn test_chat_list_start_search() {
    let mut window = create_test_chat_list_window();
    window.update(Action::ChatListSearch);
    assert!(
        window.search_mode,
        "Search mode should be true after ChatListSearch"
    );
}

#[test]
fn test_chat_list_stop_search() {
    let mut window = create_test_chat_list_window();
    window.update(Action::ChatListSearch);
    assert!(window.search_mode);

    window.update(Action::ChatListUnselect);
    assert!(
        !window.search_mode,
        "Search mode should be false after Esc/Unselect"
    );
}

#[test]
fn test_chat_list_filtering_basic() {
    let mut window = create_test_chat_list_window();
    setup_chats(&mut window, &["Alice", "Bob", "Charlie", "David"]);

    // Start search
    window.update(Action::ChatListSearch);
    assert!(window.search_mode);

    // Type 'a' to filter
    let modifiers = Modifiers::from(KeyModifiers::empty());
    window.update(Action::Key(KeyCode::Char('a'), modifiers));
    window.update(Action::ChatListSortWithString("a".to_string()));

    // Simulate filtering result (visible_chats is updated in update() via rebuild_visible_chats)
    window.visible_chats = vec![
        create_mock_chat(1, "Alice"),
        create_mock_chat(3, "Charlie"),
        create_mock_chat(4, "David"),
    ];
    window.sort("a".to_string());

    // Filter should match "Alice" and "Charlie" (contains 'a')
    // Note: Actual filtering happens in draw(), but we can test the sort_string
    assert_eq!(window.sort_string, Some("a".to_string()));
}

#[test]
fn test_chat_list_filtering_fuzzy() {
    let mut window = create_test_chat_list_window();
    setup_chats(&mut window, &["Alice", "Bob", "Charlie", "David"]);

    window.update(Action::ChatListSearch);
    let modifiers = Modifiers::from(KeyModifiers::empty());

    // Type 'al' - should match "Alice"
    window.update(Action::Key(KeyCode::Char('a'), modifiers.clone()));
    window.update(Action::Key(KeyCode::Char('l'), modifiers.clone()));
    window.update(Action::ChatListSortWithString("al".to_string()));

    assert_eq!(window.sort_string, Some("al".to_string()));
    assert_eq!(window.search_input, "al");
}

#[test]
fn test_chat_list_search_backspace() {
    let mut window = create_test_chat_list_window();
    window.update(Action::ChatListSearch);

    let modifiers = Modifiers::from(KeyModifiers::empty());
    window.update(Action::Key(KeyCode::Char('a'), modifiers.clone()));
    window.update(Action::Key(KeyCode::Char('b'), modifiers.clone()));
    assert_eq!(window.search_input, "ab");

    window.update(Action::Key(KeyCode::Backspace, modifiers));
    assert_eq!(window.search_input, "a");
}

#[test]
fn test_chat_list_search_exit_with_enter() {
    let mut window = create_test_chat_list_window();
    window.update(Action::ChatListSearch);
    assert!(window.search_mode);

    let modifiers = Modifiers::from(KeyModifiers::empty());
    window.update(Action::Key(KeyCode::Enter, modifiers));
    assert!(!window.search_mode, "Search should exit with Enter");
}

#[test]
fn test_chat_list_search_exit_with_esc() {
    let mut window = create_test_chat_list_window();
    window.update(Action::ChatListSearch);
    assert!(window.search_mode);

    window.update(Action::ChatListUnselect);
    assert!(!window.search_mode, "Search should exit with Esc/Unselect");
}

#[test]
fn test_chat_list_navigation_in_search_mode() {
    let mut window = create_test_chat_list_window();
    setup_chats(&mut window, &["Alice", "Bob", "Charlie"]);

    window.update(Action::ChatListSearch);
    let modifiers = Modifiers::from(KeyModifiers::empty());

    // Type to filter
    window.update(Action::Key(KeyCode::Char('a'), modifiers.clone()));
    window.update(Action::ChatListSortWithString("a".to_string()));

    // Navigate with arrow keys
    window.update(Action::Key(KeyCode::Down, modifiers.clone()));
    // Navigation should work in search mode
    // Note: Actual navigation state is managed by ListState which is harder to test directly
    assert!(window.search_mode, "Should still be in search mode");
}

#[test]
fn test_chat_list_restore_sort() {
    let mut window = create_test_chat_list_window();
    window.update(Action::ChatListSortWithString("test".to_string()));
    assert_eq!(window.sort_string, Some("test".to_string()));

    window.update(Action::ChatListRestoreSort);
    assert_eq!(window.sort_string, None, "Sort should be restored to None");
}

#[test]
fn test_chat_list_search_clear_on_exit() {
    let mut window = create_test_chat_list_window();
    window.update(Action::ChatListSearch);

    let modifiers = Modifiers::from(KeyModifiers::empty());
    window.update(Action::Key(KeyCode::Char('t'), modifiers.clone()));
    window.update(Action::Key(KeyCode::Char('e'), modifiers.clone()));
    window.update(Action::Key(KeyCode::Char('s'), modifiers.clone()));
    window.update(Action::Key(KeyCode::Char('t'), modifiers));
    assert_eq!(window.search_input, "test");

    window.update(Action::ChatListUnselect);
    assert!(
        window.search_input.is_empty(),
        "Search input should be cleared on exit"
    );
    assert_eq!(
        window.sort_string, None,
        "Sort string should be cleared on exit"
    );
}

#[test]
fn test_chat_list_left_click_when_not_focused_returns_none() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;

    let mut window = create_test_chat_list_window();
    setup_chats(&mut window, &["Alice", "Bob"]);
    window.focused = false;
    window.last_list_area = Some(Rect::new(0, 0, 20, 10));

    let mouse = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 5,
        row: 2,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    let result = window.handle_mouse_events(mouse).unwrap();

    assert_eq!(
        result, None,
        "Left click on chat list when not focused should not open a chat (focus first)"
    );
}

#[test]
fn test_chat_list_left_click_on_item_when_focused_selects_and_returns_open() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;

    let mut window = create_test_chat_list_window();
    setup_chats(&mut window, &["Alice", "Bob", "Charlie"]);
    window.focused = true;
    // List content: first item at row content_y = list_area.y+2 = 2; second at row 3.
    window.last_list_area = Some(Rect::new(0, 0, 20, 10));
    window.chat_list_state.select(Some(0));

    let mouse = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 5,
        row: 3, // second visible row = index 1 (Bob); first item at row 2
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    let result = window.handle_mouse_events(mouse).unwrap();

    assert_eq!(
        result,
        Some(Action::ChatListOpen),
        "Left click on item when focused should open that chat (as with Enter)"
    );
    assert_eq!(
        window.chat_list_state.selected(),
        Some(1),
        "Clicked row (index 1) should be selected"
    );
}
