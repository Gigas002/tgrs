use super::*;
use crate::ui::tests::create_test_app_context;
use ratatui::{Terminal, backend::TestBackend};

#[test]
fn lists_the_pane_toggles_first() {
    let menu = LeaderMenu::new(create_test_app_context());
    let entries = menu.entries();
    assert_eq!(
        entries.first(),
        Some(&("1".to_owned(), "Toggle the chat list".to_owned()))
    );
    // Named keys like Space come last
    assert_eq!(entries.last().map(|(key, _)| key.as_str()), Some("Space"));
}

#[test]
fn draws_only_while_open() {
    let mut menu = LeaderMenu::new(create_test_app_context());
    let draw = |menu: &mut LeaderMenu| {
        let mut terminal = Terminal::new(TestBackend::new(60, 16)).unwrap();
        terminal
            .draw(|frame| menu.draw(frame, frame.area()).unwrap())
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
    };
    assert!(!draw(&mut menu).contains("Toggle the chat list"));
    menu.update(Action::ShowLeaderMenu);
    let screen = draw(&mut menu);
    assert!(screen.contains("Space"), "{screen}");
    assert!(screen.contains("1 Toggle the chat list"), "{screen}");
    menu.update(Action::HideLeaderMenu);
    assert!(!draw(&mut menu).contains("Toggle the chat list"));
}

fn screen_of(menu: &mut LeaderMenu, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| menu.draw(frame, frame.area()).unwrap())
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_short_terminal_shows_every_entry_in_two_columns() {
    let mut menu = LeaderMenu::new(create_test_app_context());
    menu.update(Action::ShowLeaderMenu);
    let entries = menu.entries();
    // An 80x24 terminal leaves 22 rows under the title bar and over the
    // status bar, fewer than there are entries
    assert!(entries.len() + 2 > 20);
    let screen = screen_of(&mut menu, 80, 20);
    for (key, description) in entries {
        assert!(
            screen.contains(&format!("{key} {description}")),
            "{key} {description} is missing:\n{screen}"
        );
    }
}
