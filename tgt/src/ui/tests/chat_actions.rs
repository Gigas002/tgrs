use super::*;
use crate::{app::action::Modifiers, ui::tests::create_test_app_context};
use crossterm::event::{KeyCode, KeyModifiers};
use tokio::sync::mpsc;

fn group() -> ChatState {
    ChatState {
        chat_id: 7,
        title: "Rust".to_owned(),
        kind: ChatKind::Group,
        ..ChatState::default()
    }
}

/// The menu shown for `state`, as `show` would for a chat in the store.
fn menu_for(state: ChatState) -> (ChatActions, mpsc::UnboundedReceiver<Action>) {
    let mut menu = ChatActions::new(create_test_app_context());
    let (tx, rx) = mpsc::unbounded_channel();
    menu.register_action_handler(tx).unwrap();
    menu.items = ChatActions::items_for(&state);
    menu.chat = Some(state);
    (menu, rx)
}

fn sent(rx: &mut mpsc::UnboundedReceiver<Action>) -> Vec<Action> {
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

fn select(menu: &mut ChatActions, command: ChatCommand) {
    menu.selected = menu
        .items
        .iter()
        .position(|item| item.command == command)
        .unwrap();
}

#[test]
fn labels_follow_the_chat_state() {
    let (menu, _rx) = menu_for(group());
    assert_eq!(
        menu.labels(),
        [
            "mute",
            "archive",
            "pin chat",
            "mark as unread",
            "leave group"
        ]
    );
    let (menu, _rx) = menu_for(ChatState {
        kind: ChatKind::Private,
        is_muted: true,
        is_archived: true,
        is_pinned: true,
        is_unread: true,
        ..group()
    });
    assert_eq!(
        menu.labels(),
        [
            "unmute",
            "unarchive",
            "unpin chat",
            "mark as read",
            "delete chat"
        ]
    );
    let state = ChatState {
        kind: ChatKind::Channel,
        ..group()
    };
    assert_eq!(command_label(ChatCommand::Remove, &state), "leave channel");
    assert_eq!(done_label(ChatCommand::Remove, &state), "left");
    assert_eq!(done_label(ChatCommand::ToggleMute, &state), "muted");
}

#[test]
fn an_action_runs_and_closes_the_menu() {
    let (mut menu, mut rx) = menu_for(group());
    menu.update(Action::ChatActionsNext);
    menu.update(Action::ChatActionsConfirm);
    assert_eq!(
        sent(&mut rx),
        [
            Action::HideChatActions,
            Action::RunChatCommand(ChatCommand::ToggleArchive, 7)
        ]
    );
}

#[test]
fn leaving_runs_only_when_confirmed() {
    let (mut menu, mut rx) = menu_for(group());
    select(&mut menu, ChatCommand::Remove);
    menu.update(Action::ChatActionsConfirm);
    assert!(sent(&mut rx).is_empty());
    assert_eq!(menu.armed, Some(ChatCommand::Remove));
    // Moving away asks again
    menu.update(Action::ChatActionsPrevious);
    menu.update(Action::ChatActionsNext);
    menu.update(Action::ChatActionsConfirm);
    assert!(sent(&mut rx).is_empty());
    menu.update(Action::ChatActionsConfirm);
    assert_eq!(
        sent(&mut rx),
        [
            Action::HideChatActions,
            Action::RunChatCommand(ChatCommand::Remove, 7)
        ]
    );
}

#[test]
fn chat_list_keys_run_actions_and_d_twice_leaves() {
    let (mut menu, mut rx) = menu_for(group());
    let key = |c| Action::Key(KeyCode::Char(c), Modifiers::from(KeyModifiers::NONE));
    menu.update(key('m'));
    assert_eq!(
        sent(&mut rx),
        [
            Action::HideChatActions,
            Action::RunChatCommand(ChatCommand::ToggleMute, 7)
        ]
    );

    // From the chat list, D opens the menu waiting for a second D
    let (mut menu, mut rx) = menu_for(group());
    menu.armed = Some(ChatCommand::Remove);
    select(&mut menu, ChatCommand::Remove);
    // Keys arrive normalized: shift+d is D
    menu.update(key('D'));
    assert_eq!(
        sent(&mut rx),
        [
            Action::HideChatActions,
            Action::RunChatCommand(ChatCommand::Remove, 7)
        ]
    );
    assert_eq!(
        chat_command_action(ChatCommand::Remove, 7),
        Action::ShowChatActionsFor(7, Some(ChatCommand::Remove))
    );
    assert_eq!(
        chat_command_action(ChatCommand::TogglePin, 7),
        Action::RunChatCommand(ChatCommand::TogglePin, 7)
    );
}

#[test]
fn saved_messages_cant_be_archived() {
    let (mut menu, mut rx) = menu_for(ChatState {
        kind: ChatKind::Saved,
        ..group()
    });
    select(&mut menu, ChatCommand::ToggleArchive);
    menu.update(Action::ChatActionsConfirm);
    assert_eq!(
        sent(&mut rx),
        [Action::StatusMessage(
            "Can't archive: saved messages.".into()
        )]
    );
}

#[test]
fn an_unknown_chat_shows_nothing() {
    let (mut menu, _rx) = menu_for(group());
    menu.update(Action::ShowChatActionsFor(99, None));
    assert!(menu.chat.is_none());
}
