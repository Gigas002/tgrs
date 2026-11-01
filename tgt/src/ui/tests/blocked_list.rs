use super::*;
use crate::ui::tests::create_test_app_context;
use ratatui::{Terminal, backend::TestBackend};
use tokio::sync::mpsc;

fn list() -> (BlockedList, mpsc::UnboundedReceiver<Action>) {
    let mut list = BlockedList::new(create_test_app_context());
    let (tx, rx) = mpsc::unbounded_channel();
    list.register_action_handler(tx).unwrap();
    list.update(Action::ShowBlockedList);
    (list, rx)
}

fn blocked(sender: TdMessageSender, name: &str) -> BlockedSender {
    BlockedSender {
        sender,
        name: name.to_owned(),
    }
}

fn loaded() -> Action {
    Action::BlockedListLoaded(vec![
        blocked(TdMessageSender::User(1), "Spammer"),
        blocked(TdMessageSender::Chat(-2), "Ads Channel"),
        blocked(TdMessageSender::User(3), "Troll"),
    ])
}

fn sent(rx: &mut mpsc::UnboundedReceiver<Action>) -> Vec<Action> {
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

fn screen(list: &mut BlockedList) -> String {
    let (width, height) = (60, 12);
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
fn it_loads_then_lists_who_is_blocked() {
    let (mut list, _rx) = list();
    assert!(screen(&mut list).contains("Loading…"));
    list.update(loaded());
    let shown = screen(&mut list);
    assert!(shown.contains("Blocked (3)"), "{shown}");
    // Emoji take two cells, so they're checked apart from the names
    for text in ["👤", "Spammer", "📢", "Ads Channel", "u unblocks"] {
        assert!(shown.contains(text), "{text} in {shown}");
    }
}

#[test]
fn nobody_blocked_says_so() {
    let (mut list, mut rx) = list();
    list.update(Action::BlockedListLoaded(Vec::new()));
    assert!(screen(&mut list).contains("No one is blocked"));
    list.update(Action::BlockedListUnblock);
    assert!(sent(&mut rx).is_empty());
}

#[test]
fn unblocking_takes_the_highlighted_one_off_the_list() {
    let (mut list, mut rx) = list();
    list.update(loaded());
    list.update(Action::BlockedListPrevious);
    assert_eq!(list.selected, 2, "wraps to the last");
    list.update(Action::BlockedListUnblock);
    assert_eq!(
        sent(&mut rx),
        [Action::UnblockSender(TdMessageSender::User(3))]
    );
    assert_eq!(list.names(), ["Spammer", "Ads Channel"]);
    assert_eq!(list.selected, 1, "stays on the list");

    list.update(Action::BlockedListNext);
    list.update(Action::BlockedListUnblock);
    assert_eq!(
        sent(&mut rx),
        [Action::UnblockSender(TdMessageSender::User(1))]
    );
    assert_eq!(list.names(), ["Ads Channel"]);
}

#[test]
fn a_list_loaded_after_closing_is_not_shown() {
    let (mut list, _rx) = list();
    list.update(Action::HideBlockedList);
    list.update(loaded());
    assert!(list.names().is_empty());
    assert!(screen(&mut list).trim().is_empty());
}
