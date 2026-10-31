use super::*;

fn alert(text: Option<&str>) -> MessageAlert {
    MessageAlert {
        chat_id: 1,
        message_id: 2,
        chat_title: "Rust".into(),
        sender_name: Some("Ferris".into()),
        text: text.map(Into::into),
        is_silent: false,
    }
}

#[test]
fn body_is_the_message_text() {
    assert_eq!(body(&alert(Some("hi")), true), "hi");
}

#[test]
fn body_hides_the_text_without_preview() {
    assert_eq!(body(&alert(Some("hi")), false), "New message");
    // The chat hides previews
    assert_eq!(body(&alert(None), true), "New message");
}

#[test]
fn body_cuts_long_texts() {
    let text = "a".repeat(MAX_BODY_CHARS + 10);
    let body = body(&alert(Some(&text)), true);
    assert_eq!(body.chars().count(), MAX_BODY_CHARS + 1);
    assert!(body.ends_with('…'));
}

#[test]
fn title_names_the_sender_in_groups() {
    assert_eq!(alert(None).title(), "Ferris in Rust");
    let private = MessageAlert {
        sender_name: None,
        ..alert(None)
    };
    assert_eq!(private.title(), "Rust");
}

#[test]
fn markup_is_escaped() {
    assert_eq!(
        escape_markup("<b>a & b</b>"),
        "&lt;b&gt;a &amp; b&lt;/b&gt;"
    );
}
