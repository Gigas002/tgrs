use super::*;
use crate::{
    app::action::{Action, ChatCommand},
    app::event::Event,
    ui::tests::create_test_app_context,
};

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
fn test_mouse_click_in_chat_list_when_not_focused_focuses_and_opens() {
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

    // The click focuses the chat list, then the list takes it; with no chats
    // drawn there's nothing under it to open
    assert_eq!(window.component_focused, Some(ComponentName::ChatList));
    assert_eq!(
        window.app_context.focused_component(),
        Some(ComponentName::ChatList)
    );
    assert_eq!(result.unwrap(), None);
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
    // Message actions give the focus back to the chat
    assert_eq!(window.component_focused, Some(ComponentName::Chat));
}

#[test]
fn a_pane_is_focused_from_the_start() {
    let window = create_test_core_window();
    assert_eq!(window.component_focused, Some(ComponentName::ChatList));
    assert_eq!(
        window.app_context.focused_component(),
        Some(ComponentName::ChatList)
    );
}

#[test]
fn esc_never_leaves_nothing_focused() {
    let mut window = create_test_core_window();
    for pane in [
        ComponentName::ChatList,
        ComponentName::Chat,
        ComponentName::TopicList,
        ComponentName::ArchiveList,
    ] {
        window.focus_only(Some(pane));
        window.update(Action::UnfocusComponent);
        assert_eq!(window.component_focused, Some(pane), "esc in {pane}");
    }

    // Leaving the prompt goes back to the messages
    window.focus_only(Some(ComponentName::Prompt));
    window.update(Action::UnfocusComponent);
    assert_eq!(window.component_focused, Some(ComponentName::Chat));

    // A popup goes away and a pane gets the focus
    window.update(Action::ShowCommandGuide);
    window.update(Action::UnfocusComponent);
    assert_eq!(window.active_popup, None);
    assert!(
        window
            .component_focused
            .is_some_and(|focused| !focused.is_popup())
    );
}

#[test]
fn with_the_chat_list_hidden_the_chat_is_the_fallback() {
    let mut window = create_test_core_window();
    window.update(Action::ToggleChatList);
    window.update(Action::ShowCommandGuide);
    window.update(Action::HideCommandGuide);
    assert_eq!(window.component_focused, Some(ComponentName::Chat));
}

#[test]
fn command_guide_toggles() {
    let mut window = create_test_core_window();
    window.update(Action::ShowCommandGuide);
    assert!(window.is_popup_visible(ComponentName::CommandGuide));
    window.update(Action::ShowCommandGuide);
    assert_eq!(window.active_popup, None);
    assert_eq!(window.component_focused, Some(ComponentName::ChatList));
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

fn sent_actions(window: &CoreWindow) -> Vec<Action> {
    let mut rx = window.app_context.action_rx();
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

#[test]
fn chat_actions_act_on_the_open_chat_outside_the_chat_lists() {
    let mut window = create_test_core_window();
    window.focus_only(Some(ComponentName::Chat));
    window.update(Action::ShowChatActions);
    assert_eq!(
        sent_actions(&window),
        [Action::StatusMessage("Open a chat first.".into())]
    );

    window.app_context.store().set_open_chat_id_i64(5);
    window.update(Action::ShowChatActions);
    window.update(Action::SelectedChat(ChatCommand::ToggleMute));
    window.update(Action::SelectedChat(ChatCommand::Remove));
    assert_eq!(
        sent_actions(&window),
        [
            Action::ShowChatActionsFor(5, None),
            Action::RunChatCommand(ChatCommand::ToggleMute, 5),
            Action::ShowChatActionsFor(5, Some(ChatCommand::Remove)),
        ]
    );

    window.update(Action::ShowChatActionsFor(5, None));
    assert_eq!(window.component_focused, Some(ComponentName::ChatActions));
    window.update(Action::HideChatActions);
    assert_eq!(window.component_focused, Some(ComponentName::Chat));
}

#[test]
fn the_profile_shows_the_open_chat_or_yours_and_gives_the_focus_back() {
    let mut window = create_test_core_window();
    window.focus_only(Some(ComponentName::Chat));
    window.update(Action::ShowProfile);
    window.update(Action::ShowMyProfile);
    assert_eq!(
        sent_actions(&window),
        [
            Action::StatusMessage("Open a chat first.".into()),
            Action::StatusMessage("Your profile isn't known yet.".into()),
        ]
    );

    window.app_context.store().set_open_chat_id_i64(5);
    window.app_context.store().set_me(9);
    window.update(Action::ShowProfile);
    window.update(Action::ShowMyProfile);
    assert_eq!(
        sent_actions(&window),
        [Action::ShowProfileFor(5), Action::ShowProfileFor(9)]
    );

    // Chat 5 isn't known, so it has no profile
    window.update(Action::ShowProfileFor(5));
    assert_eq!(
        sent_actions(&window),
        [Action::StatusMessage("No profile for this chat.".into())]
    );
    assert!(!window.is_popup_visible(ComponentName::Profile));

    window.update(Action::ShowProfileFor(9));
    assert_eq!(window.component_focused, Some(ComponentName::Profile));
    window.update(Action::HideProfile);
    assert_eq!(window.component_focused, Some(ComponentName::Chat));
    assert!(!window.is_popup_visible(ComponentName::Profile));
}

#[test]
fn find_chat_gives_the_focus_back_or_opens_the_chat() {
    let mut window = create_test_core_window();
    window.focus_only(Some(ComponentName::Chat));
    window.update(Action::ShowFindChat);
    assert_eq!(window.component_focused, Some(ComponentName::FindChat));
    window.update(Action::HideFindChat);
    assert_eq!(window.component_focused, Some(ComponentName::Chat));
    assert!(!window.is_popup_visible(ComponentName::FindChat));

    // Esc closes it like the other popups
    window.update(Action::ShowFindChat);
    window.update(Action::UnfocusComponent);
    assert_eq!(window.component_focused, Some(ComponentName::Chat));

    window.update(Action::ShowFindChat);
    sent_actions(&window);
    window.update(Action::OpenChatById(42));
    assert!(!window.is_popup_visible(ComponentName::FindChat));
    assert_eq!(window.app_context.store().open_chat_id().as_i64(), 42);
    assert!(sent_actions(&window).contains(&Action::GetChatHistory));
}

#[test]
fn report_and_blocked_list_popups_give_the_focus_back() {
    let mut window = create_test_core_window();
    window.focus_only(Some(ComponentName::Chat));
    window.update(Action::ShowReport(5, vec![1]));
    assert_eq!(window.component_focused, Some(ComponentName::Report));
    window.update(Action::HideReport);
    assert_eq!(window.component_focused, Some(ComponentName::Chat));

    window.update(Action::ShowBlockedList);
    assert_eq!(window.component_focused, Some(ComponentName::BlockedList));
    window.update(Action::UnfocusComponent);
    assert_eq!(window.component_focused, Some(ComponentName::Chat));
    assert!(!window.is_popup_visible(ComponentName::BlockedList));
}

#[test]
fn only_your_profile_is_edited_and_saving_shows_it_again() {
    let mut window = create_test_core_window();
    window.focus_only(Some(ComponentName::Chat));
    window.update(Action::ShowEditProfile);
    assert_eq!(
        sent_actions(&window),
        [Action::StatusMessage(
            "Your profile isn't known yet.".into()
        )]
    );
    assert!(!window.is_popup_visible(ComponentName::EditProfile));

    window.app_context.store().set_me(9);
    window.update(Action::ShowEditProfile);
    assert_eq!(window.component_focused, Some(ComponentName::EditProfile));
    window.update(Action::HideEditProfile);
    assert_eq!(window.component_focused, Some(ComponentName::Chat));

    // From your own profile: saving shows it again, with the edit
    window.update(Action::ShowProfileFor(9));
    window.update(Action::ShowEditProfile);
    assert!(!window.is_popup_visible(ComponentName::Profile));
    assert!(window.is_popup_visible(ComponentName::EditProfile));
    sent_actions(&window);
    window.update(Action::ProfileSaved);
    assert!(!window.is_popup_visible(ComponentName::EditProfile));
    assert_eq!(window.component_focused, Some(ComponentName::Chat));
    assert_eq!(
        sent_actions(&window),
        [
            Action::StatusMessage("Profile saved.".into()),
            Action::ShowMyProfile
        ]
    );
}

#[test]
fn someone_else_s_profile_can_t_be_edited() {
    let mut window = create_test_core_window();
    window.focus_only(Some(ComponentName::Chat));
    window.app_context.store().set_me(9);
    // Another chat's profile, as if it were known
    window.profile_chat = Some(5);
    window.show_popup(ComponentName::Profile, Action::ShowProfileFor(5));
    sent_actions(&window);
    window.update(Action::ShowEditProfile);
    assert_eq!(
        sent_actions(&window),
        [Action::StatusMessage(
            "Only your own profile can be edited.".into()
        )]
    );
    assert!(window.is_popup_visible(ComponentName::Profile));
}

#[test]
fn the_session_list_gives_the_focus_back() {
    let mut window = create_test_core_window();
    window.focus_only(Some(ComponentName::Chat));
    window.update(Action::ShowSessions);
    assert_eq!(window.component_focused, Some(ComponentName::Sessions));
    window.update(Action::HideSessions);
    assert_eq!(window.component_focused, Some(ComponentName::Chat));
    assert!(!window.is_popup_visible(ComponentName::Sessions));
}

#[test]
fn the_forward_popup_gives_the_focus_back() {
    let mut window = create_test_core_window();
    window.focus_only(Some(ComponentName::Chat));
    window.update(Action::ShowForward(1, vec![7]));
    assert_eq!(window.component_focused, Some(ComponentName::Forward));
    window.update(Action::HideForward);
    assert_eq!(window.component_focused, Some(ComponentName::Chat));
    assert!(!window.is_popup_visible(ComponentName::Forward));
}

#[test]
fn a_picked_file_goes_to_the_prompt_for_its_caption() {
    let mut window = create_test_core_window();
    let tx = window.app_context.action_tx().clone();
    window.register_action_handler(tx).unwrap();
    window.update(Action::ShowFileUploadExplorer);
    sent_actions(&window);
    window.update(Action::UploadFile("/tmp/a.jpg".into()));
    assert!(sent_actions(&window).contains(&Action::SendFile(
        "/tmp/a.jpg".into(),
        String::new(),
        None
    )));
}

#[test]
fn files_attached_in_the_explorer_go_to_the_prompt() {
    let mut window = create_test_core_window();
    let tx = window.app_context.action_tx().clone();
    window.register_action_handler(tx).unwrap();
    window.update(Action::ShowFileUploadExplorer);
    window.update(Action::AttachFile("/tmp/a.jpg".into()));
    sent_actions(&window);
    window.update(Action::UploadFile("/tmp/b.jpg".into()));
    assert!(sent_actions(&window).contains(&Action::SendAlbum(
        vec!["/tmp/a.jpg".into(), "/tmp/b.jpg".into()],
        String::new(),
        None
    )));
}

#[test]
fn the_downloads_give_the_focus_back() {
    let mut window = create_test_core_window();
    window.focus_only(Some(ComponentName::Chat));
    window.update(Action::ShowDownloads);
    assert_eq!(window.component_focused, Some(ComponentName::Downloads));
    window.update(Action::HideDownloads);
    assert_eq!(window.component_focused, Some(ComponentName::Chat));
}

#[test]
fn the_storage_popup_gives_the_focus_back() {
    let mut window = create_test_core_window();
    window.focus_only(Some(ComponentName::Chat));
    window.update(Action::ShowStorage);
    assert_eq!(window.component_focused, Some(ComponentName::Storage));
    window.update(Action::HideStorage);
    assert_eq!(window.component_focused, Some(ComponentName::Chat));
}

#[test]
fn the_notification_settings_give_the_focus_back() {
    let mut window = create_test_core_window();
    window.focus_only(Some(ComponentName::Chat));
    window.update(Action::ShowNotificationSettings);
    assert_eq!(
        window.component_focused,
        Some(ComponentName::NotificationSettings)
    );
    window.update(Action::HideNotificationSettings);
    assert_eq!(window.component_focused, Some(ComponentName::Chat));
}

#[test]
fn the_privacy_settings_give_the_focus_back() {
    let mut window = create_test_core_window();
    window.focus_only(Some(ComponentName::Chat));
    window.update(Action::ShowPrivacy);
    assert_eq!(window.component_focused, Some(ComponentName::Privacy));
    window.update(Action::HidePrivacy);
    assert_eq!(window.component_focused, Some(ComponentName::Chat));
}

#[test]
fn closing_the_command_guide_gives_the_focus_back() {
    let mut window = create_test_core_window();
    window.focus_only(Some(ComponentName::Chat));
    window.update(Action::ShowCommandGuide);
    assert_eq!(window.component_focused, Some(ComponentName::CommandGuide));
    window.update(Action::HideCommandGuide);
    assert_eq!(window.component_focused, Some(ComponentName::Chat));

    // Opened from the leader menu: back where the menu was opened
    window.focus_only(Some(ComponentName::Prompt));
    window.update(Action::ShowLeaderMenu);
    window.update(Action::ShowCommandGuide);
    window.update(Action::HideLeaderMenu);
    window.update(Action::HideCommandGuide);
    assert_eq!(window.component_focused, Some(ComponentName::Prompt));
}
