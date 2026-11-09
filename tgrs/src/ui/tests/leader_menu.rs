use super::*;
use crate::ui::tests::{click, create_test_app_context, wheel};
use ratatui::{Terminal, backend::TestBackend};
use tokio::sync::mpsc;

fn menu() -> (LeaderMenu, mpsc::UnboundedReceiver<Action>) {
    let mut menu = LeaderMenu::new(create_test_app_context());
    let (tx, rx) = mpsc::unbounded_channel();
    menu.register_action_handler(tx).unwrap();
    menu.update(Action::ShowLeaderMenu);
    (menu, rx)
}

fn sent(rx: &mut mpsc::UnboundedReceiver<Action>) -> Vec<Action> {
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

#[test]
fn lists_the_pane_toggles_first() {
    let menu = LeaderMenu::new(create_test_app_context());
    let entries = menu.entries();
    assert_eq!(
        entries.first(),
        Some(&LeaderEntry {
            key: "1".to_owned(),
            description: "Toggle the chat list".to_owned(),
            action: Action::ToggleChatList,
        })
    );
    // Named keys like Space come last
    assert_eq!(
        entries.last().map(|entry| entry.key.as_str()),
        Some("Space")
    );
    // The keys that move through the menu aren't entries
    assert!(
        entries
            .iter()
            .all(|entry| !LeaderMenu::navigates(&entry.action))
    );
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
    for LeaderEntry {
        key, description, ..
    } in entries
    {
        assert!(
            screen.contains(&format!("{key} {description}")),
            "{key} {description} is missing:\n{screen}"
        );
    }
}

#[test]
fn arrows_move_the_highlight_and_enter_runs_it() {
    let (mut menu, mut rx) = menu();
    assert_eq!(menu.selected, 0);
    menu.update(Action::LeaderMenuPrevious);
    assert_eq!(menu.selected, menu.entries().len() - 1);
    menu.update(Action::LeaderMenuNext);
    menu.update(Action::LeaderMenuNext);
    assert_eq!(menu.selected, 1);
    menu.update(Action::LeaderMenuConfirm);
    assert_eq!(
        sent(&mut rx),
        vec![Action::HideLeaderMenu, menu.entries()[1].action.clone()]
    );
    // Reopening starts from the top
    menu.update(Action::ShowLeaderMenu);
    assert_eq!(menu.selected, 0);
}

#[test]
fn the_mouse_moves_the_highlight_and_runs_entries() {
    let (mut menu, mut rx) = menu();
    screen_of(&mut menu, 80, 40);
    let popup = menu.last_layout.unwrap().popup;

    menu.handle_mouse_events(wheel(true)).unwrap();
    assert_eq!(menu.selected, 1);
    menu.handle_mouse_events(wheel(false)).unwrap();
    assert_eq!(menu.selected, 0);

    let third = click(popup.x + 3, popup.y + 3);
    menu.handle_mouse_events(third).unwrap();
    assert_eq!(menu.selected, 2);
    assert!(sent(&mut rx).is_empty());
    menu.handle_mouse_events(third).unwrap();
    assert_eq!(
        sent(&mut rx),
        vec![Action::HideLeaderMenu, menu.entries()[2].action.clone()]
    );
    assert_eq!(
        menu.handle_mouse_events(click(0, 0)).unwrap(),
        Some(Action::HideLeaderMenu)
    );
}

#[test]
fn clicks_pick_entries_in_the_right_column() {
    let (mut menu, _rx) = menu();
    let screen = screen_of(&mut menu, 80, 20);
    let layout = menu.last_layout.unwrap();
    let (half, right_x) = layout.columns.expect("two columns");
    let entry = &menu.entries()[half];
    // The right column's first entry is drawn where the click lands
    let row = usize::from(layout.popup.y + 1);
    let line: String = screen
        .lines()
        .nth(row)
        .unwrap()
        .chars()
        .skip(usize::from(right_x))
        .collect();
    assert!(line.contains(&entry.description), "{line:?}");

    menu.handle_mouse_events(click(right_x + 1, layout.popup.y + 1))
        .unwrap();
    assert_eq!(menu.selected, half);
}
