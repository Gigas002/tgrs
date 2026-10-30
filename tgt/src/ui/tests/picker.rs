use super::*;
use crate::{
    app::action::{Modifiers, ReactionChoice},
    ui::tests::create_test_app_context,
};
use crossterm::event::KeyModifiers;
use ratatui::{Terminal, backend::TestBackend};
use tokio::sync::mpsc;

fn picker() -> (Picker, mpsc::UnboundedReceiver<Action>) {
    let mut picker = Picker::new(create_test_app_context());
    let (tx, rx) = mpsc::unbounded_channel();
    picker.register_action_handler(tx).unwrap();
    (picker, rx)
}

fn sent(rx: &mut mpsc::UnboundedReceiver<Action>) -> Vec<Action> {
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

fn reactions() -> PickerKind {
    PickerKind::Reactions {
        message_id: 7,
        reactions: vec![
            ReactionChoice {
                emoji: "👍".into(),
                chosen: true,
            },
            ReactionChoice {
                emoji: "❤".into(),
                chosen: false,
            },
        ],
    }
}

#[test]
fn picking_a_chosen_reaction_removes_it() {
    let (mut picker, mut rx) = picker();
    picker.update(Action::ShowPicker(reactions()));
    picker.update(Action::PickerConfirm);
    assert_eq!(
        sent(&mut rx),
        vec![
            Action::HidePicker,
            Action::SetReaction(7, "👍".into(), false)
        ]
    );
}

#[test]
fn number_keys_pick_directly() {
    let (mut picker, mut rx) = picker();
    picker.update(Action::ShowPicker(reactions()));
    picker.update(Action::Key(
        KeyCode::Char('2'),
        Modifiers::from(KeyModifiers::NONE),
    ));
    assert_eq!(
        sent(&mut rx),
        vec![Action::HidePicker, Action::SetReaction(7, "❤".into(), true)]
    );
    // No entry for 3: nothing happens
    picker.update(Action::Key(
        KeyCode::Char('3'),
        Modifiers::from(KeyModifiers::NONE),
    ));
    assert!(sent(&mut rx).is_empty());
}

#[test]
fn links_open_the_picked_one() {
    let (mut picker, mut rx) = picker();
    let links = vec!["https://a.example".to_owned(), "b.example".to_owned()];
    picker.update(Action::ShowPicker(PickerKind::Links(links)));
    picker.update(Action::PickerPrevious);
    picker.update(Action::PickerConfirm);
    assert_eq!(
        sent(&mut rx),
        vec![Action::HidePicker, Action::OpenUrl("b.example".into())]
    );
    picker.update(Action::HidePicker);
    assert!(picker.kind().is_none());
}

#[test]
fn shortcuts_are_one_to_nine_then_zero() {
    assert_eq!(indexed_shortcut(0), Some('1'));
    assert_eq!(indexed_shortcut(8), Some('9'));
    assert_eq!(indexed_shortcut(9), Some('0'));
    assert_eq!(indexed_shortcut(10), None);
}

#[test]
fn draws_a_titled_list_with_the_selection_marker() {
    let (mut picker, _rx) = picker();
    picker.update(Action::ShowPicker(reactions()));
    let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();
    terminal
        .draw(|frame| picker.draw(frame, frame.area()).unwrap())
        .unwrap();
    let text: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(text.contains("Choose reaction"), "{text}");
    assert!(text.contains("▸ 1 👍"), "{text}");
    assert!(text.contains("remove"), "{text}");
}
