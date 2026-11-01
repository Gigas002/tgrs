use super::*;
use crate::{app::action::Modifiers, ui::tests::create_test_app_context};
use crossterm::event::{KeyCode, KeyModifiers};
use libtgt::model::ProfileChange;
use ratatui::{Terminal, backend::TestBackend};
use tokio::sync::mpsc;

fn ada() -> ProfileEdit {
    ProfileEdit {
        first_name: "Ada".into(),
        last_name: "Lovelace".into(),
        bio: "Poet of science".into(),
        username: "ada_l".into(),
    }
}

/// The popup shown and loaded with [`ada`]'s profile.
fn popup() -> (EditProfile, mpsc::UnboundedReceiver<Action>) {
    let mut popup = EditProfile::new(create_test_app_context());
    let (tx, rx) = mpsc::unbounded_channel();
    popup.register_action_handler(tx).unwrap();
    popup.update(Action::ShowEditProfile);
    popup.fill_with(ada());
    popup.status = Status::Hint;
    (popup, rx)
}

fn sent(rx: &mut mpsc::UnboundedReceiver<Action>) -> Vec<Action> {
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

fn key(code: KeyCode) -> Action {
    Action::Key(code, Modifiers::from(KeyModifiers::NONE))
}

fn ctrl_u() -> Action {
    Action::Key(KeyCode::Char('u'), Modifiers::from(KeyModifiers::CONTROL))
}

fn type_text(popup: &mut EditProfile, text: &str) {
    for c in text.chars() {
        popup.update(key(KeyCode::Char(c)));
    }
}

fn screen(popup: &mut EditProfile) -> String {
    let (width, height) = (66, 10);
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
fn fields_are_edited_one_at_a_time() {
    let (mut popup, _rx) = popup();
    assert_eq!(popup.field, ProfileField::FirstName);
    type_text(&mut popup, "!");
    assert_eq!(popup.text(ProfileField::FirstName), "Ada!");

    popup.update(Action::EditProfileNextField);
    popup.update(Action::EditProfileNextField);
    assert_eq!(popup.field, ProfileField::Bio);
    popup.update(ctrl_u());
    popup.update(Action::Paste("Analyst".into()));
    assert_eq!(popup.text(ProfileField::Bio), "Analyst");
    assert_eq!(popup.text(ProfileField::LastName), "Lovelace");

    popup.update(Action::EditProfilePreviousField);
    popup.update(Action::EditProfilePreviousField);
    popup.update(Action::EditProfilePreviousField);
    assert_eq!(popup.field, ProfileField::Username);
}

#[test]
fn confirming_saves_only_what_changed() {
    let (mut popup, mut rx) = popup();
    popup.update(Action::EditProfileNextField);
    popup.update(ctrl_u());
    type_text(&mut popup, "King");
    popup.update(Action::EditProfileConfirm);
    assert_eq!(
        sent(&mut rx),
        [Action::SaveProfile(vec![ProfileChange::Name {
            first_name: "Ada".into(),
            last_name: "King".into(),
        }])]
    );
    assert_eq!(popup.status, Status::Saving);

    // Saving waits: no typing, moving or saving twice
    type_text(&mut popup, "x");
    popup.update(Action::EditProfileNextField);
    popup.update(Action::EditProfileConfirm);
    assert_eq!(popup.text(ProfileField::LastName), "King");
    assert_eq!(popup.field, ProfileField::LastName);
    assert!(sent(&mut rx).is_empty());
}

#[test]
fn confirming_with_nothing_changed_just_closes() {
    let (mut popup, mut rx) = popup();
    popup.update(Action::EditProfileConfirm);
    assert_eq!(sent(&mut rx), [Action::HideEditProfile]);
}

#[test]
fn a_problem_moves_to_its_field_without_saving() {
    let (mut popup, mut rx) = popup();
    popup.update(Action::EditProfilePreviousField);
    popup.update(ctrl_u());
    type_text(&mut popup, "a-b");
    popup.update(Action::EditProfileNextField);
    popup.update(ctrl_u());
    popup.update(Action::EditProfileConfirm);
    assert!(sent(&mut rx).is_empty());
    // The first name comes first
    assert_eq!(popup.field, ProfileField::FirstName);
    assert!(matches!(popup.status, Status::Error(_)));
    assert!(screen(&mut popup).contains("The first name can't be empty"));

    // Typing clears the error
    type_text(&mut popup, "Ada");
    assert_eq!(popup.status, Status::Hint);
    popup.update(Action::EditProfileConfirm);
    assert_eq!(popup.field, ProfileField::Username);
    assert!(screen(&mut popup).contains("A username has 5-32 letters"));
}

#[test]
fn a_refusal_from_telegram_points_at_its_field() {
    let (mut popup, mut rx) = popup();
    popup.update(Action::EditProfilePreviousField);
    popup.update(ctrl_u());
    type_text(&mut popup, "durov");
    popup.update(Action::EditProfileConfirm);
    sent(&mut rx);
    popup.update(Action::EditProfileNextField);
    popup.update(Action::EditProfileFailed(
        Some(ProfileField::Username),
        "@durov is taken".into(),
    ));
    assert_eq!(popup.field, ProfileField::Username);
    assert!(screen(&mut popup).contains("@durov is taken"));
    // What was typed is kept to fix it
    assert_eq!(popup.text(ProfileField::Username), "durov");
    type_text(&mut popup, "_fan");
    popup.update(Action::EditProfileConfirm);
    assert_eq!(
        sent(&mut rx),
        [Action::SaveProfile(vec![ProfileChange::Username(
            "durov_fan".into()
        )])]
    );
}

#[test]
fn nothing_is_edited_until_the_bio_is_loaded() {
    let mut popup = EditProfile::new(create_test_app_context());
    let (tx, mut rx) = mpsc::unbounded_channel();
    popup.register_action_handler(tx).unwrap();
    // Your full info isn't in the test store
    popup.update(Action::ShowEditProfile);
    assert_eq!(popup.status, Status::Loading);
    type_text(&mut popup, "x");
    popup.update(Action::EditProfileConfirm);
    assert_eq!(popup.text(ProfileField::FirstName), "");
    assert!(sent(&mut rx).is_empty());
    assert!(screen(&mut popup).contains("Loading…"));

    popup.update(Action::EditProfileLoaded);
    assert_eq!(popup.status, Status::Hint);
    type_text(&mut popup, "x");
    assert_eq!(popup.text(ProfileField::FirstName), "x");

    // A failed load still lets the rest be edited
    popup.update(Action::ShowEditProfile);
    popup.update(Action::EditProfileFailed(
        None,
        "Can't load your bio".into(),
    ));
    type_text(&mut popup, "y");
    assert_eq!(popup.text(ProfileField::FirstName), "y");
}

#[test]
fn a_reload_keeps_edits_in_progress() {
    let (mut popup, _rx) = popup();
    type_text(&mut popup, "!");
    popup.update(Action::EditProfileLoaded);
    assert_eq!(popup.text(ProfileField::FirstName), "Ada!");
}

#[test]
fn the_popup_shows_every_field_and_the_hint() {
    let (mut popup, _rx) = popup();
    let screen = screen(&mut popup);
    assert!(screen.contains("Edit your profile"), "{screen}");
    assert!(screen.contains("First name   Ada"), "{screen}");
    assert!(screen.contains(" Last name   Lovelace"), "{screen}");
    assert!(screen.contains("       Bio   Poet of science"), "{screen}");
    assert!(screen.contains("  Username  @ada_l"), "{screen}");
    assert!(screen.contains(HINT), "{screen}");
}

#[test]
fn clicks_pick_a_field_or_close_outside() {
    let (mut popup, _rx) = popup();
    screen(&mut popup);
    let popup_area = popup.last_popup.unwrap();
    let click = |column, row| MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };
    let bio_row = popup_area.y + 3;
    assert_eq!(
        popup
            .handle_mouse_events(click(popup_area.x + 5, bio_row))
            .unwrap(),
        None
    );
    assert_eq!(popup.field, ProfileField::Bio);
    assert_eq!(
        popup.handle_mouse_events(click(0, 0)).unwrap(),
        Some(Action::HideEditProfile)
    );
}
