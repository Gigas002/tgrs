use super::*;
use crate::ui::tests::{click, create_test_app_context, draw_component, wheel};
use tokio::sync::mpsc;

fn popup() -> (SendAsPopup, mpsc::UnboundedReceiver<Action>) {
    let mut popup = SendAsPopup::new(create_test_app_context());
    let (tx, rx) = mpsc::unbounded_channel();
    popup.register_action_handler(tx).unwrap();
    (popup, rx)
}

fn sent(rx: &mut mpsc::UnboundedReceiver<Action>) -> Vec<Action> {
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

#[test]
fn photos_are_highlighted_first() {
    let (mut popup, mut rx) = popup();
    popup.update(Action::ShowSendAs(2, 0));
    assert!(popup.visible);
    popup.update(Action::SendAsConfirm);
    assert_eq!(sent(&mut rx), [Action::SendAsChosen(SendAs::Photos)]);
}

#[test]
fn the_highlight_moves_and_wraps() {
    let (mut popup, mut rx) = popup();
    popup.update(Action::ShowSendAs(1, 0));
    popup.update(Action::SendAsNext);
    popup.update(Action::SendAsConfirm);
    assert_eq!(sent(&mut rx), [Action::SendAsChosen(SendAs::Files)]);
    popup.update(Action::SendAsNext);
    popup.update(Action::SendAsConfirm);
    assert_eq!(sent(&mut rx), [Action::SendAsChosen(SendAs::Photos)]);
    popup.update(Action::SendAsPrevious);
    popup.update(Action::SendAsConfirm);
    assert_eq!(sent(&mut rx), [Action::SendAsChosen(SendAs::Files)]);
}

#[test]
fn showing_again_starts_on_photos() {
    let (mut popup, mut rx) = popup();
    popup.update(Action::ShowSendAs(1, 0));
    popup.update(Action::SendAsNext);
    popup.update(Action::HideSendAs);
    assert!(!popup.visible);
    popup.update(Action::ShowSendAs(1, 0));
    popup.update(Action::SendAsConfirm);
    assert_eq!(sent(&mut rx), [Action::SendAsChosen(SendAs::Photos)]);
}

#[test]
fn a_hidden_popup_sends_nothing() {
    let (mut popup, mut rx) = popup();
    popup.update(Action::SendAsConfirm);
    assert!(sent(&mut rx).is_empty());
}

#[test]
fn the_title_counts_the_files() {
    let (mut popup, _rx) = popup();
    popup.update(Action::ShowSendAs(1, 0));
    assert_eq!(popup.title(), "Send the image as");
    popup.update(Action::ShowSendAs(3, 0));
    assert_eq!(popup.title(), "Send 3 images as");
    popup.update(Action::ShowSendAs(1, 1));
    assert_eq!(popup.title(), "Send 1 image (and 1 other file) as");
    popup.update(Action::ShowSendAs(2, 3));
    assert_eq!(popup.title(), "Send 2 images (and 3 other files) as");
}

#[test]
fn the_keys_come_from_the_keymap() {
    let (popup, _rx) = popup();
    assert_eq!(popup.key_label(SendAs::Photos), "p");
    assert_eq!(popup.key_label(SendAs::Files), "d");
}

#[test]
fn clicks_pick_and_outside_closes() {
    let (mut popup, mut rx) = popup();
    popup.update(Action::ShowSendAs(1, 0));
    draw_component(&mut popup, 80, 20);
    let (area, _) = popup.last_popup.unwrap();
    // The wheel moves to files; a click on them sends
    popup.handle_mouse_events(wheel(true)).unwrap();
    popup
        .handle_mouse_events(click(area.x + 2, area.y + 2))
        .unwrap();
    assert_eq!(sent(&mut rx), [Action::SendAsChosen(SendAs::Files)]);
    assert_eq!(
        popup.handle_mouse_events(click(0, 0)).unwrap(),
        Some(Action::HideSendAs)
    );
}
