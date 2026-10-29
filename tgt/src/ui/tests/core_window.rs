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
        window.show_search_overlay,
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
        !window.show_search_overlay,
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
    assert!(window.show_search_overlay);
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
    assert!(window.show_command_guide, "Command guide should be visible");
    assert_eq!(
        window.component_focused,
        Some(ComponentName::CommandGuide),
        "Command guide should be focused"
    );

    // Show theme selector - should hide command guide (both show action and focus change)
    window.update(Action::ShowThemeSelector);
    assert!(
        window.show_theme_selector,
        "Theme selector should be visible"
    );
    assert!(
        !window.show_command_guide,
        "Command guide should be hidden when theme selector is shown"
    );
    assert_eq!(
        window.component_focused,
        Some(ComponentName::ThemeSelector),
        "Theme selector should be focused"
    );

    // Show search overlay - should hide theme selector
    window.update(Action::ChatWindowSearch);
    assert!(
        window.show_search_overlay,
        "Search overlay should be visible"
    );
    assert!(
        !window.show_theme_selector,
        "Theme selector should be hidden when search overlay is shown"
    );
    assert!(
        !window.show_command_guide,
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
    assert!(window.show_command_guide, "Command guide should be visible");

    // Manually show theme selector and focus it (simulating direct focus)
    window.show_theme_selector = true;
    window.update(Action::FocusComponent(ComponentName::ThemeSelector));

    // Command guide should be hidden because we focused away from it (popup to popup)
    assert!(
        !window.show_command_guide,
        "Command guide should be hidden when focusing theme selector"
    );
    assert_eq!(
        window.component_focused,
        Some(ComponentName::ThemeSelector),
        "Theme selector should be focused"
    );
}

#[test]
fn test_focus_component_hides_previous_popup() {
    let mut window = create_test_core_window();

    // Show command guide
    window.update(Action::ShowCommandGuide);
    assert!(window.show_command_guide, "Command guide should be visible");
    assert_eq!(
        window.component_focused,
        Some(ComponentName::CommandGuide),
        "Command guide should be focused"
    );

    // Focus ChatList - should hide command guide since it was a popup
    window.update(Action::FocusComponent(ComponentName::ChatList));
    assert!(
        !window.show_command_guide,
        "Command guide should be hidden when focusing another component"
    );
    assert_eq!(
        window.component_focused,
        Some(ComponentName::ChatList),
        "ChatList should be focused"
    );

    // Show theme selector
    window.update(Action::ShowThemeSelector);
    assert!(
        window.show_theme_selector,
        "Theme selector should be visible"
    );

    // Focus Chat - should hide theme selector
    window.update(Action::FocusComponent(ComponentName::Chat));
    assert!(
        !window.show_theme_selector,
        "Theme selector should be hidden when focusing Chat"
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
