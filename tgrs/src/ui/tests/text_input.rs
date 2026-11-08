use super::*;
use crossterm::event::KeyModifiers;

fn press(input: &mut TextInput, code: KeyCode) -> bool {
    input.handle_key(code, Modifiers::from(KeyModifiers::NONE))
}

fn typed(text: &str) -> TextInput {
    let mut input = TextInput::default();
    input.insert(text);
    input
}

#[test]
fn keys_edit_at_the_cursor_and_say_when_the_text_changed() {
    let mut input = typed("héllo");
    assert!(!press(&mut input, KeyCode::Left));
    assert!(press(&mut input, KeyCode::Backspace));
    assert_eq!(input.text(), "hélo");
    assert!(press(&mut input, KeyCode::Delete));
    assert_eq!(input.text(), "hél");
    press(&mut input, KeyCode::Home);
    assert!(
        !press(&mut input, KeyCode::Backspace),
        "nothing before the start"
    );
    assert!(press(&mut input, KeyCode::Char('>')));
    press(&mut input, KeyCode::End);
    assert!(!press(&mut input, KeyCode::Delete), "nothing after the end");
    assert!(!press(&mut input, KeyCode::Right));
    assert_eq!(input.text(), ">hél");
}

#[test]
fn ctrl_u_clears_up_to_the_cursor() {
    let mut input = typed("one two");
    for _ in 0..3 {
        press(&mut input, KeyCode::Left);
    }
    assert!(input.handle_key(KeyCode::Char('u'), Modifiers::from(KeyModifiers::CONTROL)));
    assert_eq!(input.text(), "two");
}

#[test]
fn inserted_text_stays_on_one_line() {
    assert_eq!(typed("a\nb\tc").text(), "abc");
}

#[test]
fn the_view_scrolls_to_keep_the_cursor_in_it() {
    let input = typed("abcdefghij");
    assert_eq!(input.visible(20), ("abcdefghij".to_owned(), 10));
    let (shown, column) = input.visible(5);
    assert_eq!(column, 4);
    assert_eq!(shown, "ghij");

    let mut input = typed("abcdefghij");
    press(&mut input, KeyCode::Home);
    assert_eq!(input.visible(5), ("abcdefghij".to_owned(), 0));
}
