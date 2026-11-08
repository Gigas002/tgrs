use super::*;
use crate::ui::tests::create_test_app_context;
use ratatui::{Terminal, backend::TestBackend};

fn hints_with_focus(focus: Option<ComponentName>) -> String {
    let app_context = create_test_app_context();
    app_context.set_focused_component(focus);
    let mut bar = StatusBar::new(app_context);
    let mut terminal = Terminal::new(TestBackend::new(100, 1)).unwrap();
    terminal
        .draw(|frame| bar.draw(frame, frame.area()).unwrap())
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
        .trim_end()
        .to_owned()
}

#[test]
fn the_hints_follow_the_focus() {
    // In the prompt Space and q type, Enter sends
    let prompt = hints_with_focus(Some(ComponentName::Prompt));
    assert!(prompt.contains("Enter send"), "{prompt}");
    assert!(
        !prompt.contains("menu") && !prompt.contains("quit"),
        "{prompt}"
    );

    let chat = hints_with_focus(Some(ComponentName::Chat));
    assert!(chat.contains("Enter message actions"), "{chat}");
    // The shortest key: q, not Ctrl+c
    assert!(chat.contains("q quit"), "{chat}");

    let list = hints_with_focus(Some(ComponentName::ChatList));
    assert!(list.contains("Enter open"), "{list}");

    // Popups show their own keys
    assert_eq!(hints_with_focus(Some(ComponentName::Sessions)), "");
}
