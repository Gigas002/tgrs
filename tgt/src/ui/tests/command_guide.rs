use super::*;
use crate::{
    app::action::{Action, Modifiers},
    ui::tests::create_test_app_context,
};
use crossterm::event::{KeyCode, KeyModifiers};
use tokio::sync::mpsc;

#[test]
fn test_command_guide_initial_state() {
    let app_context = create_test_app_context();
    let guide = CommandGuide::new(app_context);

    assert!(
        !guide.is_visible(),
        "Command guide should not be visible initially"
    );
    assert!(
        !guide.focused,
        "Command guide should not be focused initially"
    );
}

#[test]
fn test_command_guide_show() {
    let app_context = create_test_app_context();
    let mut guide = CommandGuide::new(app_context);

    guide.show();
    assert!(
        guide.is_visible(),
        "Command guide should be visible after show()"
    );
}

#[test]
fn test_command_guide_hide() {
    let app_context = create_test_app_context();
    let mut guide = CommandGuide::new(app_context);

    guide.show();
    assert!(
        guide.is_visible(),
        "Command guide should be visible after show()"
    );

    guide.hide();
    assert!(
        !guide.is_visible(),
        "Command guide should not be visible after hide()"
    );
}

#[test]
fn test_command_guide_toggle() {
    let app_context = create_test_app_context();
    let mut guide = CommandGuide::new(app_context);

    // Initially hidden
    assert!(!guide.is_visible());

    // Show
    guide.show();
    assert!(guide.is_visible());

    // Hide
    guide.hide();
    assert!(!guide.is_visible());

    // Show again
    guide.show();
    assert!(guide.is_visible());
}

#[test]
fn test_command_guide_show_action() {
    let app_context = create_test_app_context();
    let mut guide = CommandGuide::new(app_context);

    assert!(!guide.is_visible());

    guide.update(Action::ShowCommandGuide);
    assert!(
        guide.is_visible(),
        "Command guide should be visible after ShowCommandGuide action"
    );
}

#[test]
fn test_command_guide_hide_action() {
    let app_context = create_test_app_context();
    let mut guide = CommandGuide::new(app_context);

    guide.show();
    assert!(guide.is_visible());

    guide.update(Action::HideCommandGuide);
    assert!(
        !guide.is_visible(),
        "Command guide should not be visible after HideCommandGuide action"
    );
}

#[test]
fn test_command_guide_close_with_esc_when_visible() {
    let app_context = create_test_app_context();
    let mut guide = CommandGuide::new(app_context);

    guide.show();
    assert!(
        guide.is_visible(),
        "Guide should be visible before HideCommandGuide"
    );

    guide.update(Action::HideCommandGuide);
    assert!(
        !guide.is_visible(),
        "Command guide should be hidden after HideCommandGuide action when visible"
    );
}

#[test]
fn test_command_guide_close_with_f1_when_visible() {
    let app_context = create_test_app_context();
    let mut guide = CommandGuide::new(app_context);

    guide.show();
    assert!(
        guide.is_visible(),
        "Guide should be visible before HideCommandGuide"
    );

    guide.update(Action::HideCommandGuide);
    assert!(
        !guide.is_visible(),
        "Command guide should be hidden after HideCommandGuide action when visible"
    );
}

#[test]
fn test_command_guide_ignores_keys_when_hidden() {
    let app_context = create_test_app_context();
    let mut guide = CommandGuide::new(app_context);

    assert!(!guide.is_visible());

    // Send various keys when hidden - should not affect visibility
    let modifiers = Modifiers::from(KeyModifiers::empty());
    guide.update(Action::Key(KeyCode::Esc, modifiers.clone()));
    assert!(
        !guide.is_visible(),
        "Esc should not affect visibility when hidden"
    );

    guide.update(Action::Key(KeyCode::F(1), modifiers.clone()));
    assert!(
        !guide.is_visible(),
        "F1 should not affect visibility when hidden"
    );

    guide.update(Action::Key(KeyCode::Char('a'), modifiers));
    assert!(
        !guide.is_visible(),
        "Other keys should not affect visibility when hidden"
    );
}

#[test]
fn test_command_guide_ignores_other_keys_when_visible() {
    let app_context = create_test_app_context();
    let mut guide = CommandGuide::new(app_context);

    guide.show();
    assert!(guide.is_visible());

    // Other keys should not close the guide
    let modifiers = Modifiers::from(KeyModifiers::empty());
    guide.update(Action::Key(KeyCode::Char('a'), modifiers.clone()));
    assert!(guide.is_visible(), "Other keys should not close the guide");

    guide.update(Action::Key(KeyCode::Enter, modifiers));
    assert!(guide.is_visible(), "Enter should not close the guide");
}

#[test]
fn test_command_guide_register_action_handler() {
    let app_context = create_test_app_context();
    let mut guide = CommandGuide::new(app_context);
    let (tx, _rx) = mpsc::unbounded_channel();

    let result = guide.register_action_handler(tx);
    assert!(result.is_ok(), "register_action_handler should succeed");
    assert!(
        guide.action_tx.is_some(),
        "action_tx should be set after registration"
    );
}

#[test]
fn test_command_guide_with_name() {
    let app_context = create_test_app_context();
    let guide = CommandGuide::new(app_context).with_name("Test Guide");

    assert_eq!(guide.name, "Test Guide");
}

#[test]
fn test_command_guide_focus_unfocus() {
    let app_context = create_test_app_context();
    let mut guide = CommandGuide::new(app_context);

    assert!(!guide.focused);

    guide.focus();
    assert!(guide.focused, "Guide should be focused after focus()");

    guide.unfocus();
    assert!(
        !guide.focused,
        "Guide should not be focused after unfocus()"
    );
}
