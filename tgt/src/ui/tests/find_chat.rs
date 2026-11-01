use super::*;
use crate::ui::tests::create_test_app_context;
use crossterm::event::KeyModifiers;
use ratatui::{Terminal, backend::TestBackend};
use tokio::sync::mpsc;

fn popup() -> (FindChat, mpsc::UnboundedReceiver<Action>) {
    let mut popup = FindChat::new(create_test_app_context());
    let (tx, rx) = mpsc::unbounded_channel();
    popup.register_action_handler(tx).unwrap();
    popup.update(Action::ShowFindChat);
    (popup, rx)
}

fn sent(rx: &mut mpsc::UnboundedReceiver<Action>) -> Vec<Action> {
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

fn key(code: KeyCode) -> Action {
    Action::Key(code, Modifiers::from(KeyModifiers::NONE))
}

fn type_text(popup: &mut FindChat, text: &str) {
    for c in text.chars() {
        popup.update(key(KeyCode::Char(c)));
    }
}

fn screen(popup: &mut FindChat) -> String {
    let (width, height) = (60, 8);
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
fn typing_edits_the_input_at_the_cursor() {
    let (mut popup, _rx) = popup();
    type_text(&mut popup, "drov");
    popup.update(key(KeyCode::Left));
    popup.update(key(KeyCode::Left));
    popup.update(key(KeyCode::Left));
    type_text(&mut popup, "u");
    assert_eq!(popup.input, "durov");
    popup.update(key(KeyCode::Delete));
    assert_eq!(popup.input, "duov");
    type_text(&mut popup, "r");
    popup.update(key(KeyCode::End));
    type_text(&mut popup, "!");
    popup.update(key(KeyCode::Backspace));
    assert_eq!(popup.input, "durov");
    popup.update(key(KeyCode::Home));
    popup.update(key(KeyCode::Backspace));
    assert_eq!(popup.input, "durov", "nothing before the start");

    popup.update(Action::Paste("\n@x".into()));
    assert_eq!(popup.input, "@xdurov", "a paste stays on one line");
    popup.update(Action::Key(
        KeyCode::Char('u'),
        Modifiers::from(KeyModifiers::CONTROL),
    ));
    popup.update(key(KeyCode::End));
    popup.update(Action::Key(
        KeyCode::Char('u'),
        Modifiers::from(KeyModifiers::CONTROL),
    ));
    assert_eq!(popup.input, "");
}

#[test]
fn enter_looks_up_a_valid_username_once() {
    let (mut popup, mut rx) = popup();
    type_text(&mut popup, "https://t.me/durov");
    popup.update(Action::FindChatConfirm);
    popup.update(Action::FindChatConfirm);
    assert_eq!(sent(&mut rx), [Action::FindChat("durov".into())]);
    assert_eq!(popup.status, Status::Searching);
    assert!(screen(&mut popup).contains("Searching…"));

    // Typing waits for the answer
    type_text(&mut popup, "x");
    assert_eq!(popup.input, "https://t.me/durov");
}

#[test]
fn an_invalid_username_is_not_looked_up() {
    let (mut popup, mut rx) = popup();
    type_text(&mut popup, "a b");
    popup.update(Action::FindChatConfirm);
    assert!(sent(&mut rx).is_empty());
    assert_eq!(popup.status, Status::Error(INVALID.to_owned()));
    assert!(screen(&mut popup).contains(INVALID));

    // Typing again brings the hint back
    popup.update(key(KeyCode::Backspace));
    assert_eq!(popup.status, Status::Hint);
}

#[test]
fn a_failed_lookup_is_shown_and_can_be_retried() {
    let (mut popup, mut rx) = popup();
    type_text(&mut popup, "nobody_here");
    popup.update(Action::FindChatConfirm);
    popup.update(Action::FindChatFailed("No one is @nobody_here".into()));
    assert!(screen(&mut popup).contains("No one is @nobody_here"));
    popup.update(Action::FindChatConfirm);
    assert_eq!(
        sent(&mut rx),
        [
            Action::FindChat("nobody_here".into()),
            Action::FindChat("nobody_here".into())
        ]
    );
}

#[test]
fn showing_it_again_starts_empty() {
    let (mut popup, _rx) = popup();
    type_text(&mut popup, "durov");
    popup.update(Action::FindChatFailed("No".into()));
    popup.update(Action::ShowFindChat);
    assert_eq!(popup.input, "");
    assert_eq!(popup.cursor, 0);
    assert_eq!(popup.status, Status::Hint);
}

#[test]
fn a_long_input_scrolls_to_keep_the_cursor_in_view() {
    let (mut popup, _rx) = popup();
    popup.focus();
    type_text(&mut popup, &"x".repeat(80));
    type_text(&mut popup, "END");
    assert!(screen(&mut popup).contains("END"));
}

#[test]
fn failures_say_what_went_wrong() {
    assert_eq!(
        failure_message("ghost", "USERNAME_NOT_OCCUPIED"),
        "No one is @ghost"
    );
    assert_eq!(
        failure_message("ghost", "Chat not found"),
        "No one is @ghost"
    );
    assert_eq!(
        failure_message("ghost", "USERNAME_INVALID"),
        "@ghost isn't a valid username"
    );
    assert_eq!(
        failure_message("ghost", "Too Many Requests"),
        "Can't find @ghost: Too Many Requests"
    );
}

#[test]
fn clicking_outside_closes_it() {
    let (mut popup, _rx) = popup();
    screen(&mut popup);
    let click = |column, row| MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };
    assert_eq!(popup.handle_mouse_events(click(30, 3)).unwrap(), None);
    assert_eq!(
        popup.handle_mouse_events(click(0, 0)).unwrap(),
        Some(Action::HideFindChat)
    );
}
