use super::*;
use crate::{
    app::action::Modifiers,
    ui::tests::{create_mock_media_message, create_mock_message, create_test_app_context},
};
use crossterm::event::{KeyCode, KeyModifiers};
use tokio::sync::mpsc;

fn menu_with(messages: Vec<MessageEntry>) -> (MessageActions, mpsc::UnboundedReceiver<Action>) {
    let app_context = create_test_app_context();
    app_context
        .store()
        .open_chat_messages()
        .insert_messages(messages);
    let mut menu = MessageActions::new(app_context);
    let (tx, rx) = mpsc::unbounded_channel();
    menu.register_action_handler(tx).unwrap();
    (menu, rx)
}

fn sent(rx: &mut mpsc::UnboundedReceiver<Action>) -> Vec<Action> {
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

fn disabled(menu: &MessageActions, command: MessageCommand) -> Option<&'static str> {
    menu.items
        .iter()
        .find(|item| item.command == command)
        .unwrap()
        .disabled
}

#[test]
fn others_text_message_disables_own_and_media_actions() {
    let (mut menu, _rx) = menu_with(vec![create_mock_message(1, "hello")]);
    menu.update(Action::ShowMessageActionsFor(1));
    assert!(menu.visible);
    assert_eq!(menu.items.len(), MessageCommand::ALL.len());
    assert_eq!(disabled(&menu, MessageCommand::Reply), None);
    assert_eq!(disabled(&menu, MessageCommand::Copy), None);
    assert_eq!(
        disabled(&menu, MessageCommand::Edit),
        Some("not your message")
    );
    assert_eq!(
        disabled(&menu, MessageCommand::DeleteForEveryone),
        Some("not your message")
    );
    assert_eq!(disabled(&menu, MessageCommand::OpenMedia), Some("no media"));
    assert_eq!(
        disabled(&menu, MessageCommand::DownloadMedia),
        Some("no media")
    );
}

#[test]
fn own_media_message_enables_media_and_delete() {
    let (mut menu, _rx) = menu_with(vec![]);
    let me = 1;
    menu.app_context.store().set_me(me);
    menu.app_context
        .store()
        .open_chat_messages()
        .insert_messages([create_mock_media_message(5, "archive.zip")]);
    menu.update(Action::ShowMessageActionsFor(5));
    for command in [
        MessageCommand::OpenMedia,
        MessageCommand::ViewMedia,
        MessageCommand::DownloadMedia,
        MessageCommand::DeleteForMe,
    ] {
        assert_eq!(disabled(&menu, command), None, "{command:?}");
    }
    // Media captions can't be edited as text
    assert_eq!(disabled(&menu, MessageCommand::Edit), Some("media message"));
}

#[test]
fn confirm_closes_the_menu_then_runs_the_command() {
    let (mut menu, mut rx) = menu_with(vec![create_mock_message(1, "hello")]);
    menu.update(Action::ShowMessageActionsFor(1));
    // Reply is first and always available
    assert_eq!(menu.selected, 0);
    menu.update(Action::MessageActionsConfirm);
    assert_eq!(
        sent(&mut rx),
        vec![
            Action::HideMessageActions,
            Action::RunMessageCommand(MessageCommand::Reply, 1),
        ]
    );
}

#[test]
fn disabled_actions_report_why_instead_of_running() {
    let (mut menu, mut rx) = menu_with(vec![create_mock_message(1, "hello")]);
    menu.update(Action::ShowMessageActionsFor(1));
    let open = menu
        .items
        .iter()
        .position(|item| item.command == MessageCommand::OpenMedia)
        .unwrap();
    menu.activate(open);
    assert_eq!(
        sent(&mut rx),
        vec![Action::StatusMessage("Can't open media: no media.".into())]
    );
}

#[test]
fn chat_keys_run_actions_directly() {
    let (mut menu, mut rx) = menu_with(vec![create_mock_message(1, "hello")]);
    menu.update(Action::ShowMessageActionsFor(1));
    // `y` copies in the bundled [chat] keymap
    menu.update(Action::Key(
        KeyCode::Char('y'),
        Modifiers::from(KeyModifiers::NONE),
    ));
    assert_eq!(
        sent(&mut rx),
        vec![
            Action::HideMessageActions,
            Action::RunMessageCommand(MessageCommand::Copy, 1),
        ]
    );
}

#[test]
fn selection_wraps_around() {
    let (mut menu, _rx) = menu_with(vec![create_mock_message(1, "hello")]);
    menu.update(Action::ShowMessageActionsFor(1));
    menu.update(Action::MessageActionsPrevious);
    assert_eq!(menu.selected, menu.items.len() - 1);
    menu.update(Action::MessageActionsNext);
    assert_eq!(menu.selected, 0);
}

#[test]
fn hiding_keeps_the_message_selected_in_the_chat() {
    let (mut menu, _rx) = menu_with(vec![create_mock_message(1, "hello")]);
    menu.update(Action::ShowMessageActionsFor(1));
    menu.update(Action::HideMessageActions);
    assert!(!menu.visible);
    assert_eq!(menu.app_context.store().jump_target_message_id_i64(), 1);
}

#[test]
fn unknown_message_does_not_open() {
    let (mut menu, _rx) = menu_with(vec![]);
    menu.update(Action::ShowMessageActionsFor(42));
    assert!(!menu.visible);
}
