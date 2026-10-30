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
