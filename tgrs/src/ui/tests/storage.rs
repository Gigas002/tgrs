use super::*;
use crate::ui::tests::create_test_app_context;
use ratatui::{Terminal, backend::TestBackend};
use tokio::sync::mpsc;

fn popup() -> (Storage, mpsc::UnboundedReceiver<Action>) {
    let mut popup = Storage::new(create_test_app_context());
    let (tx, rx) = mpsc::unbounded_channel();
    popup.register_action_handler(tx).unwrap();
    popup.update(Action::ShowStorage);
    (popup, rx)
}

fn chat(chat_id: i64, title: &str, size: i64) -> ChatStorage {
    ChatStorage {
        chat_id,
        title: title.to_owned(),
        size,
        count: 3,
    }
}

/// Two chats with files and some files of no chat.
fn loaded() -> Action {
    Action::StorageLoaded(StorageUsage {
        files_size: 6 * 1024 * 1024,
        file_count: 9,
        database_size: 1024 * 1024,
        log_size: 0,
        chats: vec![
            chat(10, "Rust", 4 * 1024 * 1024),
            chat(20, "Family", 1024 * 1024),
            chat(0, "", 1024 * 1024),
        ],
    })
}

fn sent(rx: &mut mpsc::UnboundedReceiver<Action>) -> Vec<Action> {
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

fn screen(popup: &mut Storage) -> String {
    let (width, height) = (100, 12);
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
fn usage_shows_once_loaded() {
    let (mut popup, _rx) = popup();
    assert!(screen(&mut popup).contains("Loading…"));
    popup.update(loaded());
    let screen = screen(&mut popup);
    assert!(screen.contains("Storage · 7.0 MiB"), "{screen}");
    assert!(
        screen.contains("Files 6.0 MiB (9 files) · Database 1.0 MiB · Log 0 B"),
        "{screen}"
    );
    assert!(screen.contains("Rust · 4.0 MiB · 3 files"), "{screen}");
    assert!(screen.contains("Other files · 1.0 MiB"), "{screen}");
    assert_eq!(popup.chat_ids(), [10, 20, 0]);
}

#[test]
fn clearing_everything_takes_a_second_press() {
    let (mut popup, mut rx) = popup();
    popup.update(loaded());
    popup.update(Action::StorageClearAll);
    assert!(sent(&mut rx).is_empty());
    assert!(screen(&mut popup).contains("again to delete all cached files"));
    popup.update(Action::StorageClearAll);
    assert_eq!(sent(&mut rx), [Action::ClearCache(None)]);
    // Loading again until the new usage comes
    assert!(screen(&mut popup).contains("Loading…"));
}

#[test]
fn clearing_a_chat_takes_a_second_press_on_the_same_chat() {
    let (mut popup, mut rx) = popup();
    popup.update(loaded());
    popup.update(Action::StorageClearChat);
    // Moving away disarms it
    popup.update(Action::StorageNext);
    popup.update(Action::StorageClearChat);
    assert!(sent(&mut rx).is_empty());
    popup.update(Action::StorageClearChat);
    assert_eq!(sent(&mut rx), [Action::ClearCache(Some(20))]);
}

#[test]
fn nothing_to_clear_says_so() {
    let (mut popup, mut rx) = popup();
    popup.update(Action::StorageLoaded(StorageUsage::default()));
    popup.update(Action::StorageClearAll);
    popup.update(Action::StorageClearChat);
    assert_eq!(
        sent(&mut rx),
        [Action::StatusMessage("No cached files.".into())]
    );
}

#[test]
fn usage_for_a_closed_popup_is_dropped() {
    let (mut popup, _rx) = popup();
    popup.update(Action::HideStorage);
    popup.update(loaded());
    assert!(popup.chat_ids().is_empty());
}
