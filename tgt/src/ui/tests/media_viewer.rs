use super::*;
use crate::ui::tests::{create_mock_media_message, create_mock_message, create_test_app_context};
use tokio::sync::mpsc;

fn viewer_with(messages: Vec<MessageEntry>) -> (MediaViewer, mpsc::UnboundedReceiver<Action>) {
    let app_context = create_test_app_context();
    app_context
        .store()
        .open_chat_messages()
        .insert_messages(messages);
    let mut viewer = MediaViewer::new(app_context);
    let (tx, rx) = mpsc::unbounded_channel();
    viewer.register_action_handler(tx).unwrap();
    (viewer, rx)
}

fn sent(rx: &mut mpsc::UnboundedReceiver<Action>) -> Vec<Action> {
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

#[test]
fn open_and_download_act_on_the_shown_message() {
    let (mut viewer, mut rx) = viewer_with(vec![create_mock_media_message(3, "a.zip")]);
    viewer.update(Action::ShowMediaViewer(3));
    assert!(viewer.visible);
    viewer.update(Action::MediaViewerOpen);
    viewer.update(Action::MediaViewerDownload);
    assert_eq!(
        sent(&mut rx),
        vec![Action::OpenMedia(3), Action::DownloadMedia(3)]
    );
}

#[test]
fn navigation_skips_messages_without_media() {
    let (mut viewer, mut rx) = viewer_with(vec![
        create_mock_media_message(1, "old.zip"),
        create_mock_message(2, "text"),
        create_mock_media_message(3, "new.zip"),
    ]);
    viewer.update(Action::ShowMediaViewer(3));
    viewer.update(Action::MediaViewerPrevious);
    assert_eq!(viewer.message_id, Some(1));
    viewer.update(Action::MediaViewerNext);
    assert_eq!(viewer.message_id, Some(3));

    viewer.update(Action::MediaViewerNext);
    assert_eq!(viewer.message_id, Some(3));
    assert_eq!(
        sent(&mut rx),
        vec![Action::StatusMessage(
            "No newer media in loaded messages.".into()
        )]
    );
}

#[test]
fn hiding_selects_the_last_viewed_message() {
    let (mut viewer, _rx) = viewer_with(vec![create_mock_media_message(3, "a.zip")]);
    viewer.update(Action::ShowMediaViewer(3));
    viewer.update(Action::HideMediaViewer);
    assert!(!viewer.visible);
    assert_eq!(viewer.app_context.store().jump_target_message_id_i64(), 3);
}

#[test]
fn hint_lists_the_bundled_keys() {
    let (viewer, _rx) = viewer_with(vec![]);
    assert_eq!(
        viewer.hint(),
        "Enter/o open · d download · Up/k previous · Down/j next · Esc/q close"
    );
}

#[test]
fn details_describe_the_file() {
    let (viewer, _rx) = viewer_with(vec![]);
    let message = create_mock_media_message(3, "a.zip");
    let text: Vec<String> = viewer
        .details(message.media().unwrap())
        .iter()
        .map(|line| line.to_string())
        .collect();
    assert_eq!(
        text,
        [
            "Type: File",
            "File: a.zip",
            "Size: 2.0 KiB",
            "MIME type: application/zip",
            "Cached: not downloaded yet",
        ]
    );
}
