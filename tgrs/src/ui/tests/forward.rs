use super::*;
use crate::{
    app::action::Modifiers,
    ui::tests::{create_mock_chat, create_test_app_context},
};
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use tokio::sync::mpsc;

/// The popup forwarding messages 7 and 8 of chat 1, with three chats.
fn popup() -> (Forward, mpsc::UnboundedReceiver<Action>) {
    let mut popup = Forward::new(create_test_app_context());
    let (tx, rx) = mpsc::unbounded_channel();
    popup.register_action_handler(tx).unwrap();
    popup.update(Action::ShowForward(1, vec![7, 8]));
    popup.set_chats(vec![
        create_mock_chat(10, "Saved Messages"),
        create_mock_chat(20, "Rust Users"),
        create_mock_chat(30, "Ada Lovelace"),
    ]);
    (popup, rx)
}

fn sent(rx: &mut mpsc::UnboundedReceiver<Action>) -> Vec<Action> {
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

fn type_text(popup: &mut Forward, text: &str) {
    for c in text.chars() {
        popup.update(Action::Key(
            KeyCode::Char(c),
            Modifiers::from(KeyModifiers::NONE),
        ));
    }
}

fn screen(popup: &mut Forward) -> String {
    let (width, height) = (64, 20);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| popup.draw(frame, frame.area()).unwrap())
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .chunks(usize::from(width))
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn matching_keeps_the_order_without_a_query_and_ranks_with_one() {
    let names = ["Saved Messages", "Rust Users", "Ada Lovelace", "Rustaceans"];
    assert_eq!(matching(&names, ""), [0, 1, 2, 3]);
    assert_eq!(matching(&names, "  "), [0, 1, 2, 3]);
    let found = matching(&names, "rust");
    assert_eq!(found.len(), 2);
    assert!(found.contains(&1) && found.contains(&3));
    assert_eq!(matching(&names, "lovelace"), [2]);
    assert!(matching(&names, "zzz").is_empty());
}

#[test]
fn typing_finds_the_chat_and_enter_forwards_there() {
    let (mut popup, mut rx) = popup();
    assert_eq!(popup.shown_names().len(), 3);
    type_text(&mut popup, "ada");
    // Fuzzy: "sAveD messAges" matches too, but less well
    assert_eq!(popup.shown_names(), ["Ada Lovelace", "Saved Messages"]);
    popup.update(Action::ForwardConfirm);
    assert_eq!(
        sent(&mut rx),
        [
            Action::HideForward,
            Action::ForwardMessages(ForwardRequest {
                to_chat_id: 30,
                to_chat_name: "Ada Lovelace".into(),
                from_chat_id: 1,
                message_ids: vec![7, 8],
                send_copy: false,
            })
        ]
    );
}

#[test]
fn the_highlight_moves_and_tab_drops_the_author() {
    let (mut popup, mut rx) = popup();
    popup.update(Action::ForwardPrevious);
    assert_eq!(popup.selected, 2);
    popup.update(Action::ForwardNext);
    popup.update(Action::ForwardNext);
    popup.update(Action::ForwardToggleCopy);
    assert!(screen(&mut popup).contains("[x] without the original author"));
    popup.update(Action::ForwardConfirm);
    match sent(&mut rx).pop() {
        Some(Action::ForwardMessages(request)) => {
            assert_eq!(request.to_chat_id, 20);
            assert!(request.send_copy);
        }
        other => panic!("not forwarded: {other:?}"),
    }
}

#[test]
fn nothing_is_forwarded_when_no_chat_matches() {
    let (mut popup, mut rx) = popup();
    type_text(&mut popup, "zzz");
    assert!(screen(&mut popup).contains("No chat matches"));
    popup.update(Action::ForwardConfirm);
    assert!(sent(&mut rx).is_empty());
}

#[test]
fn the_popup_names_how_many_messages_and_the_chats() {
    let (mut popup, _rx) = popup();
    let screen = screen(&mut popup);
    assert!(screen.contains("Forward 2 messages"), "{screen}");
    assert!(screen.contains("To:"), "{screen}");
    assert!(
        screen.contains("[ ] without the original author"),
        "{screen}"
    );
    assert!(screen.contains("Rust Users"), "{screen}");
    assert!(screen.contains(HINT), "{screen}");

    popup.update(Action::ShowForward(1, vec![7]));
    assert!(self::screen(&mut popup).contains("Forward message"));
}

#[test]
fn clicks_pick_a_chat_or_close_outside() {
    let (mut popup, _rx) = popup();
    screen(&mut popup);
    let (area, list_y, _) = popup.last_popup.unwrap();
    let click = |column, row| MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };
    popup
        .handle_mouse_events(click(area.x + 4, list_y + 1))
        .unwrap();
    assert_eq!(popup.selected, 1);
    assert_eq!(
        popup.handle_mouse_events(click(0, 0)).unwrap(),
        Some(Action::HideForward)
    );
}
