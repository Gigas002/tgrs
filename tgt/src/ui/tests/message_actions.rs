use super::*;
use crate::{
    app::action::Modifiers,
    ui::tests::{create_mock_media_message, create_mock_message, create_test_app_context},
};
use crossterm::event::{KeyCode, KeyModifiers};
use libtgt::model::{TdMessageSender, TextSpan, TextStyle};
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
    // Copy is first, like concord's menu, and the message has text
    assert_eq!(menu.selected, 0);
    menu.update(Action::MessageActionsConfirm);
    assert_eq!(
        sent(&mut rx),
        vec![
            Action::HideMessageActions,
            Action::RunMessageCommand(MessageCommand::Copy, 1),
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

fn label(menu: &MessageActions, command: MessageCommand) -> &'static str {
    menu.items
        .iter()
        .find(|item| item.command == command)
        .unwrap()
        .label
}

fn message_with_link(id: i64) -> MessageEntry {
    let link = TextStyle {
        link: true,
        ..TextStyle::default()
    };
    MessageEntry::formatted(
        id,
        TdMessageSender::User(2),
        vec![vec![
            TextSpan::plain("see "),
            TextSpan::styled("example.org", link),
        ]],
        0,
    )
}

#[test]
fn open_url_needs_a_link_and_go_to_needs_a_reply() {
    let (mut menu, _rx) = menu_with(vec![create_mock_message(1, "hi"), message_with_link(2)]);
    menu.update(Action::ShowMessageActionsFor(1));
    assert_eq!(disabled(&menu, MessageCommand::OpenLink), Some("no links"));
    assert_eq!(
        disabled(&menu, MessageCommand::GoToReply),
        Some("not a reply")
    );
    assert_eq!(disabled(&menu, MessageCommand::React), None);
    menu.update(Action::ShowMessageActionsFor(2));
    assert_eq!(disabled(&menu, MessageCommand::OpenLink), None);
}

#[test]
fn pinned_messages_offer_unpin() {
    let (mut menu, _rx) = menu_with(vec![create_mock_message(1, "hi")]);
    menu.update(Action::ShowMessageActionsFor(1));
    assert_eq!(label(&menu, MessageCommand::TogglePin), "pin message");
    menu.app_context
        .store()
        .open_chat_messages()
        .get_message_mut(1)
        .unwrap()
        .set_is_pinned(true);
    menu.update(Action::ShowMessageActionsFor(1));
    assert_eq!(label(&menu, MessageCommand::TogglePin), "unpin message");
}

#[test]
fn concord_keys_react_and_reply() {
    let (mut menu, mut rx) = menu_with(vec![create_mock_message(1, "hello")]);
    for (key, command) in [('r', MessageCommand::React), ('R', MessageCommand::Reply)] {
        menu.update(Action::ShowMessageActionsFor(1));
        menu.update(Action::Key(
            KeyCode::Char(key),
            Modifiers::from(KeyModifiers::NONE),
        ));
        assert_eq!(
            sent(&mut rx),
            vec![
                Action::HideMessageActions,
                Action::RunMessageCommand(command, 1),
            ],
            "{key}"
        );
    }
}

#[test]
fn protected_messages_can_t_be_forwarded() {
    let mut protected = create_mock_message(2, "secret");
    protected.set_can_be_forwarded(false);
    let (mut menu, _rx) = menu_with(vec![create_mock_message(1, "hello"), protected]);
    menu.update(Action::ShowMessageActionsFor(1));
    assert_eq!(disabled(&menu, MessageCommand::Forward), None);
    menu.update(Action::ShowMessageActionsFor(2));
    assert_eq!(
        disabled(&menu, MessageCommand::Forward),
        Some("protected content")
    );
}
