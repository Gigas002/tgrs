use super::*;
use crate::ui::tests::create_test_app_context;
use libtgt::model::{Media, MediaFile, MediaKind};
use ratatui::{Terminal, backend::TestBackend};
use tokio::sync::mpsc;

fn media(id: i32, name: &str) -> Media {
    Media {
        kind: MediaKind::Document,
        file: MediaFile {
            id,
            size: 1024,
            ..MediaFile::default()
        },
        file_name: name.into(),
        mime_type: String::new(),
        duration_secs: 0,
        preview: None,
        thumbnail: None,
    }
}

/// Downloads of `c.mp3` (done), `b.pdf` (paused) and `a.zip`
/// (downloading), latest first.
fn popup() -> (Downloads, mpsc::UnboundedReceiver<Action>) {
    let app_context = create_test_app_context();
    let store = app_context.store();
    store.start_transfer(&media(1, "a.zip"), TransferPurpose::Save);
    store.start_transfer(&media(2, "b.pdf"), TransferPurpose::Open);
    store.set_transfer_state(2, TransferState::Paused);
    store.start_transfer(&media(3, "c.mp3"), TransferPurpose::Save);
    store.set_transfer_state(3, TransferState::Done);
    let mut popup = Downloads::new(Arc::clone(&app_context));
    let (tx, rx) = mpsc::unbounded_channel();
    popup.register_action_handler(tx).unwrap();
    popup.update(Action::ShowDownloads);
    (popup, rx)
}

fn sent(rx: &mut mpsc::UnboundedReceiver<Action>) -> Vec<Action> {
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

fn screen(popup: &mut Downloads) -> String {
    let (width, height) = (90, 10);
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
fn downloads_show_their_progress_latest_first() {
    let (mut popup, _rx) = popup();
    assert_eq!(popup.names(), ["c.mp3", "b.pdf", "a.zip"]);
    let screen = screen(&mut popup);
    assert!(screen.contains("✓ c.mp3 · 1.0 KiB · done"), "{screen}");
    assert!(
        screen.contains("⏸ b.pdf · 0 B / 1.0 KiB · 0% · paused"),
        "{screen}"
    );
    assert!(screen.contains("⬇ a.zip"), "{screen}");
    assert!(screen.contains("p pause/resume"), "{screen}");
}

#[test]
fn p_pauses_a_download_and_resumes_a_paused_one() {
    let (mut popup, mut rx) = popup();
    popup.update(Action::DownloadsPrevious);
    popup.update(Action::DownloadsTogglePause);
    assert_eq!(sent(&mut rx), [Action::PauseDownload(1)]);
    popup.update(Action::DownloadsPrevious);
    popup.update(Action::DownloadsTogglePause);
    // It's resumed for what it was started for
    assert_eq!(
        sent(&mut rx),
        [Action::FetchMedia(2, TransferPurpose::Open)]
    );
}

#[test]
fn x_cancels_unfinished_downloads_only() {
    let (mut popup, mut rx) = popup();
    popup.update(Action::DownloadsCancel);
    assert!(matches!(
        sent(&mut rx).as_slice(),
        [Action::StatusMessage(_)]
    ));
    popup.update(Action::DownloadsNext);
    popup.update(Action::DownloadsCancel);
    assert_eq!(sent(&mut rx), [Action::CancelDownload(2)]);
}

#[test]
fn enter_opens_a_finished_download() {
    let (mut popup, mut rx) = popup();
    popup.update(Action::DownloadsOpen);
    assert_eq!(
        sent(&mut rx),
        [Action::FetchMedia(3, TransferPurpose::Open)]
    );
    popup.update(Action::DownloadsNext);
    popup.update(Action::DownloadsOpen);
    assert!(matches!(
        sent(&mut rx).as_slice(),
        [Action::StatusMessage(_)]
    ));
}

#[test]
fn the_selection_follows_its_download_and_c_clears_finished_ones() {
    let (mut popup, _rx) = popup();
    popup.update(Action::DownloadsPrevious);
    assert_eq!(popup.selected().unwrap().name, "a.zip");
    popup.update(Action::DownloadsClearFinished);
    assert_eq!(popup.names(), ["b.pdf", "a.zip"]);
    assert_eq!(popup.selected().unwrap().name, "a.zip");
}

#[test]
fn an_empty_list_says_how_to_start() {
    let mut popup = Downloads::new(create_test_app_context());
    popup.update(Action::ShowDownloads);
    assert!(screen(&mut popup).contains("No downloads yet"));
}
