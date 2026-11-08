use super::*;
use crate::ui::tests::create_test_app_context;
use libtgt::model::DeviceKind;
use ratatui::{Terminal, backend::TestBackend};
use tokio::sync::mpsc;

fn list() -> (Sessions, mpsc::UnboundedReceiver<Action>) {
    let mut list = Sessions::new(create_test_app_context());
    let (tx, rx) = mpsc::unbounded_channel();
    list.register_action_handler(tx).unwrap();
    list.update(Action::ShowSessions);
    (list, rx)
}

fn session(id: i64, application: &str) -> SessionEntry {
    SessionEntry {
        id,
        application: application.to_owned(),
        device: "Pixel 8 · Android 14".to_owned(),
        location: "Berlin, Germany".to_owned(),
        last_active: "12:30".to_owned(),
        device_kind: DeviceKind::Phone,
        ..SessionEntry::default()
    }
}

/// This app, a phone and a new sign-in from a browser.
fn loaded() -> Action {
    let mut current = session(1, "tgt 1.0");
    current.is_current = true;
    current.device_kind = DeviceKind::Computer;
    let mut new = session(3, "Telegram Web");
    new.is_unconfirmed = true;
    new.device_kind = DeviceKind::Browser;
    Action::SessionsLoaded(vec![current, session(2, "Telegram Android"), new])
}

fn sent(rx: &mut mpsc::UnboundedReceiver<Action>) -> Vec<Action> {
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

fn screen(list: &mut Sessions) -> String {
    let (width, height) = (100, 10);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| list.draw(frame, frame.area()).unwrap())
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
fn sessions_show_once_loaded() {
    let (mut list, _rx) = list();
    assert!(screen(&mut list).contains("Loading…"));
    list.update(loaded());
    let screen = screen(&mut list);
    assert!(screen.contains("Sessions (3)"), "{screen}");
    assert!(screen.contains("tgt 1.0"), "{screen}");
    assert!(screen.contains("(this app)"), "{screen}");
    assert!(
        screen.contains("Telegram Android · Pixel 8 · Android 14 · Berlin, Germany · 12:30"),
        "{screen}"
    );
    assert!(screen.contains("(new, unconfirmed)"), "{screen}");
}

#[test]
fn logging_a_session_out_takes_a_second_press() {
    let (mut list, mut rx) = list();
    list.update(loaded());
    list.update(Action::SessionsNext);
    list.update(Action::SessionsTerminate);
    assert!(sent(&mut rx).is_empty());
    assert_eq!(list.armed, Some(Armed::One(2)));
    assert!(screen(&mut list).contains("again to log it out"));

    list.update(Action::SessionsTerminate);
    assert_eq!(
        sent(&mut rx),
        [Action::TerminateSession(2, "Telegram Android".into())]
    );
    assert_eq!(list.ids(), [1, 3]);
    assert_eq!(list.selected, 1);
    assert_eq!(list.armed, None);
}

#[test]
fn moving_away_disarms() {
    let (mut list, mut rx) = list();
    list.update(loaded());
    list.update(Action::SessionsNext);
    list.update(Action::SessionsTerminate);
    list.update(Action::SessionsNext);
    list.update(Action::SessionsPrevious);
    list.update(Action::SessionsTerminate);
    assert!(sent(&mut rx).is_empty());
    assert_eq!(list.ids(), [1, 2, 3]);
}

#[test]
fn this_app_s_session_is_left_to_logout() {
    let (mut list, mut rx) = list();
    list.update(loaded());
    list.update(Action::SessionsTerminate);
    list.update(Action::SessionsTerminate);
    assert_eq!(
        sent(&mut rx),
        [
            Action::StatusMessage(CURRENT_SESSION.into()),
            Action::StatusMessage(CURRENT_SESSION.into())
        ]
    );
    assert_eq!(list.ids(), [1, 2, 3]);
}

#[test]
fn all_others_are_logged_out_on_the_second_press() {
    let (mut list, mut rx) = list();
    list.update(loaded());
    list.update(Action::SessionsNext);
    list.update(Action::SessionsTerminateOthers);
    assert!(sent(&mut rx).is_empty());
    assert!(screen(&mut list).contains("again to log out all others"));
    list.update(Action::SessionsTerminateOthers);
    assert_eq!(sent(&mut rx), [Action::TerminateOtherSessions]);
    assert_eq!(list.ids(), [1]);
    assert_eq!(list.selected, 0);

    list.update(Action::SessionsTerminateOthers);
    assert_eq!(
        sent(&mut rx),
        [Action::StatusMessage("No other sessions.".into())]
    );
}

#[test]
fn only_new_sign_ins_are_confirmed() {
    let (mut list, mut rx) = list();
    list.update(loaded());
    list.update(Action::SessionsConfirm);
    assert_eq!(
        sent(&mut rx),
        [Action::StatusMessage(
            "Only new sign-ins need confirming.".into()
        )]
    );
    list.update(Action::SessionsPrevious);
    list.update(Action::SessionsConfirm);
    assert_eq!(
        sent(&mut rx),
        [Action::ConfirmSession(3, "Telegram Web".into())]
    );
    assert!(!screen(&mut list).contains("unconfirmed"));
}

#[test]
fn a_late_answer_after_closing_is_ignored() {
    let (mut list, _rx) = list();
    list.update(Action::HideSessions);
    list.update(loaded());
    assert!(list.ids().is_empty());
    assert!(screen(&mut list).trim().is_empty());
}

#[test]
fn clicks_select_a_row_or_close_outside() {
    let (mut list, _rx) = list();
    list.update(loaded());
    screen(&mut list);
    let (popup, _) = list.last_popup.unwrap();
    let click = |column, row| MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    // The rows start under the top border
    list.handle_mouse_events(click(popup.x + 3, popup.y + 3))
        .unwrap();
    assert_eq!(list.selected, 2);
    assert_eq!(
        list.handle_mouse_events(click(0, 0)).unwrap(),
        Some(Action::HideSessions)
    );
}
