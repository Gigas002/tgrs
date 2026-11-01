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
fn test_chat_list_entry_without_last_message_has_fixed_height() {
    let app_context = create_test_app_context();
    // No last_message.
    let entry = create_mock_chat(1, "Zöli");

    let text = chat_list_item_text(&entry, &app_context, 40, false);

    let line_count = text.iter().count();
    assert_eq!(
        line_count,
        usize::from(CHAT_LIST_ITEM_ROWS),
        "Chat list entry without last message must have a fixed height (name + [No message] + spacer) for click mapping; got {}",
        line_count
    );
}

#[test]
fn test_chat_list_entry_with_empty_last_message_has_fixed_height() {
    let app_context = create_test_app_context();
    let mut entry = create_mock_chat(1, "Pandúr Katinka");
    // last_message present but with empty content (e.g. "joined Telegram" → backend sends empty message).
    entry.last_message = Some(create_mock_message(1, ""));

    let text = chat_list_item_text(&entry, &app_context, 40, false);

    let line_count = text.iter().count();
    assert_eq!(
        line_count,
        usize::from(CHAT_LIST_ITEM_ROWS),
        "Chat list entry with empty last message (e.g. joined Telegram) must have a fixed height (name + [No message] + spacer); got {}",
        line_count
    );
}

fn row_text(text: &Text<'_>) -> Vec<String> {
    text.lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect()
}

#[test]
fn rows_show_the_icon_time_preview_and_badges() {
    use libtgt::model::{ChatKind, MessageEntry, TdMessageSender};
    use ratatui::style::Style;
    use unicode_width::UnicodeWidthStr;
    let app_context = create_test_app_context();
    app_context.store().set_me(1);
    let mut entry = create_mock_chat(5, "Rust");
    entry.kind = ChatKind::Group;
    entry.is_pinned = true;
    entry.unread_count = 3;
    entry.unread_mention_count = 1;
    entry.last_message = Some(MessageEntry::plain(
        9,
        TdMessageSender::User(1),
        "see you\nlater",
        1_700_000_000,
    ));

    let text = chat_list_item_text(&entry, &app_context, 40, true);
    let rows = row_text(&text);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[2], "", "a blank spacer separates chats");
    // Selected lines are highlighted, the spacer isn't
    let selected = app_context.theme().style(HighlightGroup::SelectedRow);
    assert_eq!(text.lines[0].style, selected);
    assert_eq!(text.lines[2].style, Style::default());
    // Every row fills the width so the badges and time line up at the right
    for row in &rows[..2] {
        assert_eq!(row.width(), 40, "{row:?}");
    }
    assert!(rows[0].starts_with("▸ 👥 Rust"), "{rows:?}");
    assert!(
        rows[1].contains("You: see you"),
        "only the first line: {rows:?}"
    );
    assert!(rows[1].trim_end().ends_with("📌 @  3"), "{rows:?}");

    // Unselected rows keep the marker's width blank
    let text = chat_list_item_text(&entry, &app_context, 40, false);
    assert!(row_text(&text)[0].starts_with("  👥 Rust"));
}

#[test]
fn chats_without_messages_say_so() {
    let app_context = create_test_app_context();
    let text = chat_list_item_text(&create_mock_chat(1, "New"), &app_context, 30, false);
    assert!(row_text(&text)[1].contains("No messages"));
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
    // Rows start below the top border and padding at row 2, 3 rows each: Bob at rows 5-7.
    window.last_list_area = Some(Rect::new(1, 2, 16, 7));
    window.chat_list_state.select(Some(0));

    let mouse = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 5,
        row: 5, // first row of index 1 (Bob)
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

#[test]
fn the_archive_row_comes_first_and_opens_the_archived_chats() {
    use libtgt::model::OrderedChat;
    use libtgt::store::ARCHIVE_NAME;
    use libtgt::tdlib_rs::types::ChatPosition;
    let mut window = create_test_chat_list_window();
    let store = window.app_context.store();
    store.chats_index().insert(OrderedChat {
        chat_id: 1,
        position: ChatPosition {
            list: ChatList::Main,
            order: 1,
            is_pinned: false,
            source: None,
        },
    });
    store.archive_index().insert(OrderedChat {
        chat_id: 2,
        position: ChatPosition {
            list: ChatList::Archive,
            order: 1,
            is_pinned: false,
            source: None,
        },
    });
    window.rebuild_visible_chats();
    let kinds: Vec<ChatKind> = window.visible_chats.iter().map(|c| c.kind).collect();
    assert_eq!(kinds, [ChatKind::Archive, ChatKind::Private]);
    assert_eq!(window.visible_chats[0].chat_name, ARCHIVE_NAME);
    // No chat open: the archive row isn't taken for it
    assert_eq!(window.chat_list_state.selected(), None);

    window.chat_list_state.select(Some(0));
    window.update(Action::ChatListOpen);
    let actions: Vec<Action> = {
        let mut rx = window.app_context.action_rx();
        std::iter::from_fn(|| rx.try_recv().ok()).collect()
    };
    assert_eq!(
        actions,
        [Action::FocusComponent(ComponentName::ArchiveList)]
    );
    assert_eq!(window.app_context.store().open_chat_id().as_i64(), 0);
}

#[test]
fn the_archive_row_lists_its_chats() {
    let app_context = create_test_app_context();
    let entry = ChatListEntry {
        chat_name: "Archived chats".to_owned(),
        kind: ChatKind::Archive,
        summary: Some("Old group, Old bot".to_owned()),
        unread_count: 2,
        ..ChatListEntry::default()
    };
    let rows = row_text(&chat_list_item_text(&entry, &app_context, 40, false));
    assert!(rows[0].contains("📦 Archived chats"), "{rows:?}");
    assert!(rows[1].contains("Old group, Old bot"), "{rows:?}");
    assert!(rows[1].ends_with(" 2 "), "{rows:?}");
}

#[test]
fn chat_keys_act_on_the_selected_chat_but_not_the_archive_row() {
    use crate::app::action::ChatCommand;
    let mut window = create_test_chat_list_window();
    setup_chats(&mut window, &["Alice", "Bob"]);
    let mut archive_row = create_mock_chat(0, "Archived chats");
    archive_row.kind = ChatKind::Archive;
    window.visible_chats.insert(0, archive_row);
    let sent = |window: &ChatListWindow| -> Vec<Action> {
        let mut rx = window.app_context.action_rx();
        std::iter::from_fn(|| rx.try_recv().ok()).collect()
    };

    window.chat_list_state.select(Some(0));
    window.update(Action::ShowChatActions);
    assert!(matches!(sent(&window)[..], [Action::StatusMessage(_)]));

    window.chat_list_state.select(Some(2));
    let bob = window.visible_chats[2].chat_id;
    window.update(Action::ShowChatActions);
    window.update(Action::SelectedChat(ChatCommand::ToggleArchive));
    assert_eq!(
        sent(&window),
        [
            Action::ShowChatActionsFor(bob, None),
            Action::RunChatCommand(ChatCommand::ToggleArchive, bob)
        ]
    );

    window.update(Action::ShowProfile);
    assert_eq!(sent(&window), [Action::ShowProfileFor(bob)]);
    window.chat_list_state.select(Some(0));
    window.update(Action::ShowProfile);
    assert!(matches!(sent(&window)[..], [Action::StatusMessage(_)]));
}

#[test]
fn a_chat_marked_as_unread_has_an_empty_badge() {
    let app_context = create_test_app_context();
    let mut entry = create_mock_chat(1, "Alice");
    entry.is_marked_as_unread = true;
    let rows = row_text(&chat_list_item_text(&entry, &app_context, 30, false));
    assert!(rows[1].ends_with("  "), "{rows:?}");
    let text = chat_list_item_text(&entry, &app_context, 30, false);
    let badge = text.lines[1].spans.last().unwrap();
    assert_eq!(
        badge.style,
        app_context.theme().style(HighlightGroup::NotificationBadge)
    );
}
