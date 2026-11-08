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

fn settings() -> Notifications {
    Notifications {
        enabled: true,
        show_preview: true,
        sound: Some("message-new-instant".into()),
        quiet_hours: crate::settings::QuietHours::parse("23:00-07:30"),
        open_on_click: true,
        unread_in_title: true,
    }
}

fn at(hour: u32, minute: u32) -> chrono::NaiveTime {
    chrono::NaiveTime::from_hms_opt(hour, minute, 0).unwrap()
}

#[test]
fn quiet_hours_silence_notifications_past_midnight() {
    let settings = settings();
    assert!(is_quiet(&settings, at(23, 0)));
    assert!(is_quiet(&settings, at(3, 15)));
    assert!(is_quiet(&settings, at(7, 29)));
    assert!(!is_quiet(&settings, at(7, 30)));
    assert!(!is_quiet(&settings, at(12, 0)));
    let always = Notifications {
        quiet_hours: None,
        ..settings
    };
    assert!(!is_quiet(&always, at(3, 0)));
}

#[test]
fn silent_messages_play_no_sound() {
    let settings = settings();
    assert_eq!(sound(&alert(None), &settings), Some("message-new-instant"));
    let silent = MessageAlert {
        is_silent: true,
        ..alert(None)
    };
    assert_eq!(sound(&silent, &settings), None);
}

#[test]
fn window_title_counts_unread_messages() {
    assert_eq!(window_title(0), "tgt");
    assert_eq!(window_title(3), "tgt (3)");
}
