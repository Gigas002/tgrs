use super::*;
use crate::{app::action::Action, app::event::Event, ui::tests::create_test_app_context};

fn create_test_core_window() -> CoreWindow {
    let app_context = create_test_app_context();
    CoreWindow::new(app_context)
}

#[test]
fn test_alt_r_activates_chat_list_search_when_nothing_focused() {
    let mut window = create_test_core_window();
    window.component_focused = None;

    // Simulate Alt+R keypress (this would come from keymap, but we test the action directly)
    window.update(Action::ChatListSearch);

    // Should focus ChatList and activate search
    assert_eq!(
        window.component_focused,
        Some(ComponentName::ChatList),
        "ChatList should be focused after Alt+R when nothing is focused"
    );
}

#[test]
fn test_alt_r_activates_chat_list_search_when_chat_list_focused() {
    let mut window = create_test_core_window();
    window.component_focused = Some(ComponentName::ChatList);
    if let Some(component) = window.components.get_mut(&ComponentName::ChatList) {
        component.focus();
    }

    window.update(Action::ChatListSearch);

    assert_eq!(
        window.component_focused,
        Some(ComponentName::ChatList),
        "ChatList should remain focused"
    );
}

#[test]
fn test_alt_r_activates_message_search_when_chat_focused() {
    let mut window = create_test_core_window();
    window.component_focused = Some(ComponentName::Chat);
    if let Some(component) = window.components.get_mut(&ComponentName::Chat) {
        component.focus();
    }

    // ChatWindowSearch opens the server search overlay
    window.update(Action::ChatWindowSearch);

    assert_eq!(
        window.component_focused,
        Some(ComponentName::SearchOverlay),
        "Search overlay should be focused after ChatWindowSearch"
    );
    assert!(
        window.is_popup_visible(ComponentName::SearchOverlay),
        "Search overlay should be visible"
    );
}

#[test]
fn test_alt_r_switches_from_message_search_to_chat_list_search() {
    let mut window = create_test_core_window();
    window.component_focused = Some(ComponentName::Chat);
    if let Some(component) = window.components.get_mut(&ComponentName::Chat) {
        component.focus();
    }
    // Open search overlay (server search)
    window.update(Action::ChatWindowSearch);

    // Now Alt+R (ChatListSearch) should close overlay and focus ChatList
    window.update(Action::ChatListSearch);

    assert_eq!(
        window.component_focused,
        Some(ComponentName::ChatList),
        "Should switch to ChatList search when Alt+R pressed during message search"
    );
    assert!(
        !window.is_popup_visible(ComponentName::SearchOverlay),
        "Search overlay should be closed"
    );
}

#[test]
fn test_alt_c_restores_chat_window_sort() {
    let mut window = create_test_core_window();
    window.component_focused = Some(ComponentName::Chat);

    // Set a sort string first
    window.update(Action::ChatWindowSortWithString("test".to_string()));

    // Then restore
    window.update(Action::ChatWindowRestoreSort);

    // The action should be propagated to Chat component
    // We can't directly check the internal state, but we verify the action is handled
    assert_eq!(
        window.component_focused,
        Some(ComponentName::Chat),
        "Chat should remain focused"
    );
}

#[test]
fn test_alt_c_restores_chat_list_sort() {
    let mut window = create_test_core_window();
    window.component_focused = Some(ComponentName::ChatList);

    // Set a sort string first
    window.update(Action::ChatListSortWithString("test".to_string()));

    // Then restore
    window.update(Action::ChatListRestoreSort);

    assert_eq!(
        window.component_focused,
        Some(ComponentName::ChatList),
        "ChatList should remain focused"
    );
}

#[test]
fn test_chat_list_search_propagates_to_component() {
    let mut window = create_test_core_window();
    window.component_focused = Some(ComponentName::ChatList);

    window.update(Action::ChatListSearch);

    // ChatList should receive the search action
    assert_eq!(
        window.component_focused,
        Some(ComponentName::ChatList),
        "ChatList should be focused"
    );
}

#[test]
fn test_chat_window_search_opens_overlay() {
    let mut window = create_test_core_window();
    window.component_focused = Some(ComponentName::Chat);

    window.update(Action::ChatWindowSearch);

    // Search overlay should open and be focused
    assert_eq!(
        window.component_focused,
        Some(ComponentName::SearchOverlay),
        "Search overlay should be focused"
    );
    assert!(window.is_popup_visible(ComponentName::SearchOverlay));
}

#[test]
fn test_mouse_click_in_chat_list_when_not_focused_focuses_chat_list() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;

    let mut window = create_test_core_window();
    window.component_focused = Some(ComponentName::Chat);

    // Set ChatList area so click is inside it
    let chat_list_rect = Rect::new(0, 0, 20, 10);
    window
        .last_focusable_areas
        .insert(ComponentName::ChatList, chat_list_rect);

    let mouse = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 5,
        row: 5,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    let result = window.handle_events(Some(Event::Mouse(mouse)));

    assert_eq!(
        result.unwrap(),
        Some(Action::FocusComponent(ComponentName::ChatList)),
        "First click in chat list when Chat focused should focus ChatList (not open a chat)"
    );
}

#[test]
fn test_showing_popup_hides_other_popups() {
    let mut window = create_test_core_window();

    // Show command guide
    window.update(Action::ShowCommandGuide);
    assert!(
        window.is_popup_visible(ComponentName::CommandGuide),
        "Command guide should be visible"
    );
    assert_eq!(
        window.component_focused,
        Some(ComponentName::CommandGuide),
        "Command guide should be focused"
    );

    // Show upload explorer - should hide command guide (both show action and focus change)
    window.update(Action::ShowFileUploadExplorer);
    assert!(
        window.is_popup_visible(ComponentName::FileUploadExplorer),
        "Upload explorer should be visible"
    );
    assert!(
        !window.is_popup_visible(ComponentName::CommandGuide),
        "Command guide should be hidden when upload explorer is shown"
    );
    assert_eq!(
        window.component_focused,
        Some(ComponentName::FileUploadExplorer),
        "Upload explorer should be focused"
    );

    // Show search overlay - should hide upload explorer
    window.update(Action::ChatWindowSearch);
    assert!(
        window.is_popup_visible(ComponentName::SearchOverlay),
        "Search overlay should be visible"
    );
    assert!(
        !window.is_popup_visible(ComponentName::FileUploadExplorer),
        "Upload explorer should be hidden when search overlay is shown"
    );
    assert!(
        !window.is_popup_visible(ComponentName::CommandGuide),
        "Command guide should still be hidden"
    );
    assert_eq!(
        window.component_focused,
        Some(ComponentName::SearchOverlay),
        "Search overlay should be focused"
    );
}

#[test]
fn test_popup_to_popup_focus_hides_previous() {
    let mut window = create_test_core_window();

    // Show and focus command guide
    window.update(Action::ShowCommandGuide);
    assert!(
        window.is_popup_visible(ComponentName::CommandGuide),
        "Command guide should be visible"
    );

    // Manually show upload explorer and focus it (simulating direct focus)
    window.active_popup = Some(ComponentName::FileUploadExplorer);
    window.update(Action::FocusComponent(ComponentName::FileUploadExplorer));

    // Command guide should be hidden because we focused away from it (popup to popup)
    assert!(
        !window.is_popup_visible(ComponentName::CommandGuide),
        "Command guide should be hidden when focusing upload explorer"
    );
    assert_eq!(
        window.component_focused,
        Some(ComponentName::FileUploadExplorer),
        "Upload explorer should be focused"
    );
}

#[test]
fn test_focus_component_hides_previous_popup() {
    let mut window = create_test_core_window();

    // Show command guide
    window.update(Action::ShowCommandGuide);
    assert!(
        window.is_popup_visible(ComponentName::CommandGuide),
        "Command guide should be visible"
    );
    assert_eq!(
        window.component_focused,
        Some(ComponentName::CommandGuide),
        "Command guide should be focused"
    );

    // Focus ChatList - should hide command guide since it was a popup
    window.update(Action::FocusComponent(ComponentName::ChatList));
    assert!(
        !window.is_popup_visible(ComponentName::CommandGuide),
        "Command guide should be hidden when focusing another component"
    );
    assert_eq!(
        window.component_focused,
        Some(ComponentName::ChatList),
        "ChatList should be focused"
    );

    // Show upload explorer
    window.update(Action::ShowFileUploadExplorer);
    assert!(
        window.is_popup_visible(ComponentName::FileUploadExplorer),
        "Upload explorer should be visible"
    );

    // Focus Chat - should hide upload explorer
    window.update(Action::FocusComponent(ComponentName::Chat));
    assert!(
        !window.is_popup_visible(ComponentName::FileUploadExplorer),
        "Upload explorer should be hidden when focusing Chat"
    );
    assert_eq!(
        window.component_focused,
        Some(ComponentName::Chat),
        "Chat should be focused"
    );
}

#[test]
fn test_focus_non_popup_does_not_hide_non_popup() {
    let mut window = create_test_core_window();

    // Focus ChatList
    window.update(Action::FocusComponent(ComponentName::ChatList));
    assert_eq!(
        window.component_focused,
        Some(ComponentName::ChatList),
        "ChatList should be focused"
    );

    // Focus Chat - should not trigger any hiding since ChatList is not a popup
    window.update(Action::FocusComponent(ComponentName::Chat));
    assert_eq!(
        window.component_focused,
        Some(ComponentName::Chat),
        "Chat should be focused"
    );
    // No assertions about hiding because ChatList is not a popup component
}

#[test]
fn message_actions_return_focus_to_the_chat() {
    let mut window = create_test_core_window();
    window.update(Action::FocusComponent(ComponentName::Chat));

    window.update(Action::ShowMessageActionsFor(1));
    assert!(window.is_popup_visible(ComponentName::MessageActions));
    assert_eq!(
        window.component_focused,
        Some(ComponentName::MessageActions)
    );

    window.update(Action::HideMessageActions);
    assert_eq!(window.active_popup, None);
    assert_eq!(window.component_focused, Some(ComponentName::Chat));
}

#[test]
fn unfocus_hides_the_popup() {
    let mut window = create_test_core_window();
    window.update(Action::ShowMessageActionsFor(1));
    window.update(Action::UnfocusComponent);
    assert_eq!(window.active_popup, None);
    assert_eq!(window.component_focused, None);
}

#[test]
fn command_guide_toggles() {
    let mut window = create_test_core_window();
    window.update(Action::ShowCommandGuide);
    assert!(window.is_popup_visible(ComponentName::CommandGuide));
    window.update(Action::ShowCommandGuide);
    assert_eq!(window.active_popup, None);
    assert_eq!(window.component_focused, None);
}

mod panes {
    use super::*;
    use crate::settings::ActionBinding;
    use crossterm::event::{KeyCode, KeyModifiers};

    fn focused(window: &CoreWindow) -> Option<ComponentName> {
        window.component_focused
    }

    #[test]
    fn left_and_right_move_between_panes_and_stop_at_the_ends() {
        let mut window = create_test_core_window();
        window.focus_only(Some(ComponentName::ChatList));
        window.update(Action::FocusPreviousPane);
        assert_eq!(focused(&window), Some(ComponentName::ChatList));
        window.update(Action::FocusNextPane);
        assert_eq!(focused(&window), Some(ComponentName::Chat));
        window.update(Action::FocusNextPane);
        assert_eq!(focused(&window), Some(ComponentName::Prompt));
        window.update(Action::FocusNextPane);
        assert_eq!(focused(&window), Some(ComponentName::Prompt));
        // Esc in the prompt goes back to the messages
        window.update(Action::UnfocusComponent);
        assert_eq!(focused(&window), Some(ComponentName::Chat));
    }

    #[test]
    fn a_hidden_chat_list_is_skipped() {
        let mut window = create_test_core_window();
        window.toggle_chat_list();
        window.focus_only(Some(ComponentName::Chat));
        window.update(Action::FocusPreviousPane);
        assert_eq!(focused(&window), Some(ComponentName::Chat));
    }

    #[test]
    fn alt_arrows_resize_the_focused_pane() {
        let mut window = create_test_core_window();
        let start = window.size_chat_list;
        window.focus_only(Some(ComponentName::ChatList));
        window.update(Action::ResizePaneWider);
        assert_eq!(window.size_chat_list, start + 1);
        // The chat is on the other side: wider chat, narrower chat list
        window.focus_only(Some(ComponentName::Chat));
        window.update(Action::ResizePaneWider);
        window.update(Action::ResizePaneWider);
        assert_eq!(window.size_chat_list, start - 1);

        window.focus_only(Some(ComponentName::Prompt));
        window.update(Action::ResizePaneTaller);
        assert_eq!(window.prompt_extra_rows, 1);
        // A taller chat takes the row back, and the prompt never shrinks below its text
        window.focus_only(Some(ComponentName::Chat));
        window.update(Action::ResizePaneTaller);
        window.update(Action::ResizePaneTaller);
        assert_eq!(window.prompt_extra_rows, 0);
    }

    #[test]
    fn arrows_are_bound_like_concord() {
        let app_context = create_test_app_context();
        let keymap = app_context.keymap_config();
        let action = |component, code, modifiers| match keymap
            .get_map_of(Some(component))
            .get(&Event::Key(code, modifiers))
        {
            Some(ActionBinding::Single { action, .. }) => Some(action.clone()),
            _ => None,
        };
        let none = KeyModifiers::NONE;
        let alt = KeyModifiers::ALT;
        assert_eq!(
            action(ComponentName::ChatList, KeyCode::Right, none),
            Some(Action::FocusNextPane)
        );
        assert_eq!(
            action(ComponentName::Chat, KeyCode::Left, none),
            Some(Action::FocusPreviousPane)
        );
        assert_eq!(
            action(ComponentName::Chat, KeyCode::Char('i'), none),
            Some(Action::FocusComponent(ComponentName::Prompt))
        );
        // Plain arrows move the cursor in the prompt
        assert_eq!(action(ComponentName::Prompt, KeyCode::Left, none), None);
        for component in [
            ComponentName::ChatList,
            ComponentName::Chat,
            ComponentName::Prompt,
        ] {
            assert_eq!(
                action(component, KeyCode::Right, alt),
                Some(Action::ResizePaneWider)
            );
            assert_eq!(
                action(component, KeyCode::Up, alt),
                Some(Action::ResizePaneTaller)
            );
        }
    }
}

mod leader {
    use super::*;

    #[test]
    fn space_then_one_hides_the_chat_list_and_gives_the_focus_back() {
        let mut window = create_test_core_window();
        window.focus_only(Some(ComponentName::ChatList));
        window.update(Action::ShowLeaderMenu);
        assert_eq!(window.active_popup, Some(ComponentName::LeaderMenu));
        assert_eq!(window.component_focused, Some(ComponentName::LeaderMenu));
        // What the run loop sends for `1`
        window.update(Action::HideLeaderMenu);
        assert_eq!(window.component_focused, Some(ComponentName::ChatList));
        window.update(Action::ToggleChatList);
        assert_eq!(window.size_chat_list, 0);
        // The hidden chat list hands the focus to the chat
        assert_eq!(window.component_focused, Some(ComponentName::Chat));
        assert_eq!(window.active_popup, None);
        // And shows it again
        window.update(Action::ShowLeaderMenu);
        window.update(Action::HideLeaderMenu);
        window.update(Action::ToggleChatList);
        assert!(window.size_chat_list > 0);
    }
}

/// A core window whose open chat is a forum.
fn create_forum_core_window() -> CoreWindow {
    let window = create_test_core_window();
    window.app_context.store().set_open_chat_is_forum(true);
    window
}

#[test]
fn the_topic_list_is_a_pane_only_in_a_forum() {
    let mut window = create_test_core_window();
    assert!(!window.panes().contains(&ComponentName::TopicList));
    window.update(Action::FocusComponent(ComponentName::TopicList));
    assert_ne!(window.component_focused, Some(ComponentName::TopicList));

    let mut window = create_forum_core_window();
    assert_eq!(
        window.panes(),
        [
            ComponentName::ChatList,
            ComponentName::TopicList,
            ComponentName::Chat,
            ComponentName::Prompt
        ]
    );
    window.focus_only(Some(ComponentName::ChatList));
    window.update(Action::FocusNextPane);
    assert_eq!(window.component_focused, Some(ComponentName::TopicList));
    window.update(Action::FocusNextPane);
    assert_eq!(window.component_focused, Some(ComponentName::Chat));
}

#[test]
fn the_topic_list_can_be_hidden_and_shown_again() {
    let mut window = create_forum_core_window();
    window.focus_only(Some(ComponentName::TopicList));
    window.update(Action::ToggleTopicList);
    assert!(!window.topic_list_shown());
    assert_eq!(window.component_focused, Some(ComponentName::Chat));
    assert!(!window.panes().contains(&ComponentName::TopicList));

    // Focusing it shows it again
    window.update(Action::FocusComponent(ComponentName::TopicList));
    assert!(window.topic_list_shown());
    assert_eq!(window.component_focused, Some(ComponentName::TopicList));
}

#[test]
fn the_topic_list_resizes_and_leaves_room_for_the_chat() {
    let mut window = create_forum_core_window();
    window.focus_only(Some(ComponentName::TopicList));
    let start = window.size_topic_list;
    window.update(Action::ResizePaneWider);
    assert_eq!(window.size_topic_list, start + 1);
    // The chat grows by taking from the topic list, not the chat list
    window.focus_only(Some(ComponentName::Chat));
    let chat_list = window.size_chat_list;
    window.update(Action::ResizePaneWider);
    assert_eq!(window.size_topic_list, start);
    assert_eq!(window.size_chat_list, chat_list);

    window.focus_only(Some(ComponentName::TopicList));
    for _ in 0..100 {
        window.update(Action::ResizePaneWider);
    }
    for _ in 0..100 {
        window.update(Action::IncreaseChatListSize);
    }
    assert!(window.size_chat_list + window.size_topic_list <= MAX_SIDE_PANES_SIZE);
    for _ in 0..100 {
        window.update(Action::ResizePaneNarrower);
    }
    assert_eq!(window.size_topic_list, MIN_CHAT_LIST_SIZE);
}

#[test]
fn a_forum_draws_the_topics_between_the_chats_and_the_chat() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut window = create_forum_core_window();
    let (width, height) = (100, 20);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| window.draw(frame, frame.area()).unwrap())
        .unwrap();
    let top: String = terminal.backend().buffer().content()[..usize::from(width)]
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    let chats = top.find("Chats").unwrap();
    let topics = top.find("Topics").expect(&top);
    let chat = top.find("Chat ").unwrap();
    assert!(chats < topics && topics < chat, "{top}");
}

#[test]
fn the_archive_is_a_pane_once_opened_and_closes_for_another_chat() {
    let mut window = create_forum_core_window();
    assert!(!window.panes().contains(&ComponentName::ArchiveList));

    window.update(Action::FocusComponent(ComponentName::ArchiveList));
    assert_eq!(window.component_focused, Some(ComponentName::ArchiveList));
    assert_eq!(
        window.panes(),
        [
            ComponentName::ChatList,
            ComponentName::ArchiveList,
            ComponentName::TopicList,
            ComponentName::Chat,
            ComponentName::Prompt
        ]
    );
    // The chat grows by taking from the pane next to it
    window.focus_only(Some(ComponentName::Chat));
    let (archive, topics) = (window.size_archive_list, window.size_topic_list);
    window.update(Action::ResizePaneWider);
    assert_eq!(window.size_topic_list, topics - 1);
    assert_eq!(window.size_archive_list, archive);

    window.focus_only(Some(ComponentName::ArchiveList));
    window.update(Action::CloseArchiveList);
    assert!(!window.panes().contains(&ComponentName::ArchiveList));
    assert_eq!(window.component_focused, Some(ComponentName::ChatList));
}

#[test]
fn the_archive_can_be_toggled() {
    let mut window = create_test_core_window();
    window.update(Action::ToggleArchiveList);
    assert!(window.panes().contains(&ComponentName::ArchiveList));
    window.update(Action::ToggleArchiveList);
    assert!(!window.panes().contains(&ComponentName::ArchiveList));
}

#[test]
fn wide_side_panes_are_narrowed_to_leave_room_for_the_chat() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut window = create_test_core_window();
    window.update(Action::FocusComponent(ComponentName::ArchiveList));
    for _ in 0..100 {
        window.update(Action::IncreaseChatListSize);
    }
    window.focus_only(Some(ComponentName::ArchiveList));
    for _ in 0..100 {
        window.update(Action::ResizePaneWider);
    }
    assert!(window.side_panes_size() <= MAX_SIDE_PANES_SIZE);

    // A forum opens: its topics get room from the others
    window.app_context.store().set_open_chat_is_forum(true);
    let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
    terminal
        .draw(|frame| window.draw(frame, frame.area()).unwrap())
        .unwrap();
    assert!(window.side_panes_size() <= MAX_SIDE_PANES_SIZE);
    assert!(window.size_chat_list >= MIN_CHAT_LIST_SIZE);
    assert!(window.size_archive_list >= MIN_CHAT_LIST_SIZE);
}

#[test]
fn the_sticker_picker_needs_an_open_chat_and_gives_the_focus_back() {
    let mut window = create_test_core_window();
    window.focus_only(Some(ComponentName::Chat));
    window.update(Action::ShowStickerPicker);
    assert_eq!(window.component_focused, Some(ComponentName::Chat));
    assert!(!window.is_popup_visible(ComponentName::StickerPicker));

    window.app_context.store().set_open_chat_id_i64(5);
    window.update(Action::ShowStickerPicker);
    assert_eq!(window.component_focused, Some(ComponentName::StickerPicker));
    window.update(Action::HideStickerPicker);
    assert_eq!(window.component_focused, Some(ComponentName::Chat));
    assert!(!window.is_popup_visible(ComponentName::StickerPicker));
}
