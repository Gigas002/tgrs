use super::*;
use crate::ui::tests::create_test_app_context;
use crossterm::event::KeyModifiers;
use ratatui::{Terminal, backend::TestBackend};

fn person() -> Profile {
    Profile {
        chat_id: 5,
        kind: ChatKind::Private,
        name: "Ada Lovelace".to_owned(),
        usernames: vec!["ada".to_owned()],
        bio: "Poet of science".to_owned(),
        id: 5,
        is_loaded: true,
        ..Profile::default()
    }
}

fn text(lines: &[Line<'_>]) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect()
}

/// A popup showing your own profile (user 9), known only by its ID.
fn my_profile_popup() -> ProfilePopup {
    let app_context = create_test_app_context();
    app_context.store().set_me(9);
    let mut popup = ProfilePopup::new(app_context);
    popup.update(Action::ShowProfileFor(9));
    popup
}

fn screen(popup: &mut ProfilePopup, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| popup.draw(frame, frame.area()).unwrap())
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .chunks(usize::from(width))
        .map(|row| row.iter().map(|cell| cell.symbol()).collect())
        .collect()
}

#[test]
fn wrap_breaks_at_spaces_line_breaks_and_long_words() {
    assert_eq!(wrap("one two three", 7), ["one two", "three"]);
    assert_eq!(wrap("first\nsecond", 20), ["first", "second"]);
    assert_eq!(wrap("abcdefgh", 3), ["abc", "def", "gh"]);
    assert_eq!(wrap("", 5), [""]);
}

#[test]
fn labels_line_up_before_wrapped_values() {
    let app_context = create_test_app_context();
    let profile = Profile {
        bio: "a bio that goes on".to_owned(),
        ..person()
    };
    // Labels are 8 wide ("Username") with two spaces, leaving 10 for values
    let lines = profile_lines(app_context.theme(), &profile, 20);
    assert_eq!(
        text(&lines),
        [
            "Username  @ada",
            "     Bio  a bio that",
            "          goes on",
            "      ID  5",
        ]
    );
}

#[test]
fn a_profile_still_loading_says_so() {
    let app_context = create_test_app_context();
    let profile = Profile {
        is_loaded: false,
        ..person()
    };
    let lines = text(&profile_lines(app_context.theme(), &profile, 40));
    assert_eq!(lines.last().map(String::as_str), Some("Loading…"));
}

#[test]
fn the_title_names_the_chat_and_you() {
    assert_eq!(title(&person()), "👤 Ada Lovelace");
    let group = Profile {
        kind: ChatKind::Group,
        name: "Rust".to_owned(),
        ..person()
    };
    assert_eq!(title(&group), "👥 Rust");
    let me = Profile {
        kind: ChatKind::Saved,
        is_me: true,
        ..person()
    };
    assert_eq!(title(&me), "👤 Ada Lovelace (you)");
    assert_eq!(title(&Profile::default()), "👤 Profile");
}

#[test]
fn the_popup_shows_the_profile_until_hidden() {
    let mut popup = my_profile_popup();
    let shown = popup.shown().expect("your profile");
    assert!(shown.is_me);
    assert_eq!(shown.id, 9);

    let rows = screen(&mut popup, 70, 10);
    let screen_text = rows.join("\n");
    assert!(screen_text.contains("(you)"), "{screen_text}");
    assert!(screen_text.contains("ID  9"), "{screen_text}");
    assert!(screen_text.contains("Loading…"), "{screen_text}");

    // Details loaded for another chat leave it alone
    popup.update(Action::ProfileLoaded(4));
    assert!(popup.shown().is_some());

    popup.update(Action::HideProfile);
    assert!(popup.shown().is_none());
    assert!(
        screen(&mut popup, 70, 10)
            .iter()
            .all(|row| row.trim().is_empty())
    );
}

#[test]
fn an_unknown_chat_shows_nothing() {
    let mut popup = ProfilePopup::new(create_test_app_context());
    popup.update(Action::ShowProfileFor(42));
    assert!(popup.shown().is_none());
}

#[test]
fn scrolling_stays_within_the_lines_that_do_not_fit() {
    let mut popup = my_profile_popup();
    // "ID" and "Loading…" in a popup with room for one line
    screen(&mut popup, 70, 3);
    popup.update(Action::ProfileScrollUp);
    assert_eq!(popup.scroll, 0);
    popup.update(Action::ProfileScrollDown);
    popup.update(Action::ProfileScrollDown);
    assert_eq!(popup.scroll, 1);
    let rows = screen(&mut popup, 70, 3);
    assert!(rows[1].contains("Loading…"), "{}", rows.join("\n"));

    // Shown again: from the top
    popup.update(Action::ShowProfileFor(9));
    assert_eq!(popup.scroll, 0);
}

#[test]
fn clicking_outside_closes_the_popup() {
    let mut popup = my_profile_popup();
    screen(&mut popup, 70, 10);
    let click = |column, row| MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };
    assert_eq!(popup.handle_mouse_events(click(35, 5)).unwrap(), None);
    assert_eq!(
        popup.handle_mouse_events(click(0, 0)).unwrap(),
        Some(Action::HideProfile)
    );
}
