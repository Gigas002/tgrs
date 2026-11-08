use super::*;
use crate::ui::tests::create_test_app_context;
use libtgrs::model::{Audience, PrivacyRules};
use ratatui::{Terminal, backend::TestBackend};
use tokio::sync::mpsc;

fn popup() -> (Privacy, mpsc::UnboundedReceiver<Action>) {
    let mut popup = Privacy::new(create_test_app_context());
    let (tx, rx) = mpsc::unbounded_channel();
    popup.register_action_handler(tx).unwrap();
    popup.update(Action::ShowPrivacy);
    (popup, rx)
}

fn rules(audience: Audience) -> PrivacyRules {
    PrivacyRules {
        audience,
        ..PrivacyRules::default()
    }
}

/// Last seen for contacts, with Ferris (5) always; the phone number for
/// nobody; finding by number for everybody; read time shown.
fn loaded() -> Action {
    let mut last_seen = rules(Audience::Contacts);
    last_seen.always_users = vec![5];
    Action::PrivacyLoaded(PrivacySettings {
        rules: vec![
            (PrivacySetting::LastSeen, last_seen),
            (PrivacySetting::PhoneNumber, rules(Audience::Nobody)),
            (PrivacySetting::FindByPhoneNumber, rules(Audience::Contacts)),
        ],
        show_read_date: true,
    })
}

fn sent(rx: &mut mpsc::UnboundedReceiver<Action>) -> Vec<Action> {
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

fn screen(popup: &mut Privacy) -> String {
    let (width, height) = (110, 12);
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
fn settings_show_their_audience_and_exceptions() {
    let (mut popup, _rx) = popup();
    assert!(screen(&mut popup).contains("Loading…"));
    popup.set_person(5, "Ferris");
    popup.update(loaded());
    assert_eq!(
        popup.shown(),
        [
            "Last seen & online: My contacts (+1) · Ferris: always",
            "Phone number: Nobody",
            "Find me by number: My contacts",
            "Read time: Shown",
        ]
    );
    let screen = screen(&mut popup);
    assert!(
        screen.contains("a/n/r Ferris always/never/default"),
        "{screen}"
    );
}

#[test]
fn enter_moves_to_the_next_audience() {
    let (mut popup, mut rx) = popup();
    popup.update(loaded());
    popup.update(Action::PrivacyCycle);
    let Some(Action::SetPrivacyRules(PrivacySetting::LastSeen, saved)) = sent(&mut rx).pop() else {
        panic!("not saved");
    };
    assert_eq!(saved.audience, Audience::Nobody);
    assert_eq!(saved.always_users, [5]);
    assert_eq!(popup.shown()[0], "Last seen & online: Nobody (+1)");
    // Finding by number is never for nobody
    popup.update(Action::PrivacyNext);
    popup.update(Action::PrivacyNext);
    popup.update(Action::PrivacyCycle);
    assert_eq!(popup.shown()[2], "Find me by number: Everybody");
}

#[test]
fn the_read_time_turns_on_and_off() {
    let (mut popup, mut rx) = popup();
    popup.update(loaded());
    popup.update(Action::PrivacyPrevious);
    popup.update(Action::PrivacyCycle);
    assert_eq!(sent(&mut rx), [Action::SetShowReadDate(false)]);
    assert_eq!(popup.shown()[3], "Read time: Hidden (and others' from you)");
}

#[test]
fn the_open_chats_person_becomes_an_exception() {
    let (mut popup, mut rx) = popup();
    popup.set_person(5, "Ferris");
    popup.update(loaded());
    popup.update(Action::PrivacyNever);
    let Some(Action::SetPrivacyRules(_, saved)) = sent(&mut rx).pop() else {
        panic!("not saved");
    };
    assert_eq!(saved.never_users, [5]);
    assert!(saved.always_users.is_empty());
    popup.update(Action::PrivacyFollow);
    popup.update(Action::PrivacyFollow);
    let actions = sent(&mut rx);
    assert!(matches!(actions[0], Action::SetPrivacyRules(..)));
    assert_eq!(
        actions[1],
        Action::StatusMessage("Last seen & online follows the audience with Ferris.".into())
    );
}

#[test]
fn exceptions_need_a_person_and_a_setting_that_takes_them() {
    let (mut popup, mut rx) = popup();
    popup.update(loaded());
    popup.update(Action::PrivacyAlways);
    assert_eq!(
        sent(&mut rx),
        [Action::StatusMessage(
            "Open a private chat to make its person an exception.".into()
        )]
    );
    popup.set_person(5, "Ferris");
    popup.update(Action::PrivacyNext);
    popup.update(Action::PrivacyNext);
    popup.update(Action::PrivacyAlways);
    assert_eq!(
        sent(&mut rx),
        [Action::StatusMessage(
            "Find me by number has no exceptions.".into()
        )]
    );
}

#[test]
fn settings_for_a_closed_popup_are_dropped() {
    let (mut popup, _rx) = popup();
    popup.update(Action::HidePrivacy);
    popup.update(loaded());
    assert!(popup.shown().is_empty());
}
