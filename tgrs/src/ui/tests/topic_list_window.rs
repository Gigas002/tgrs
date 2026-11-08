use super::*;
use crate::ui::tests::create_test_app_context;
use libtgrs::model::{MessageEntry, TdMessageSender};

fn topic(topic_id: i32, name: &str, order: i64) -> TopicEntry {
    TopicEntry {
        chat_id: -100,
        topic_id,
        name: name.to_owned(),
        order,
        is_general: topic_id == 1,
        ..TopicEntry::default()
    }
}

/// A topic list of an open forum with General (1), Rust (2) and Go (3).
fn create_test_topic_list() -> TopicListWindow {
    let app_context = create_test_app_context();
    let store = app_context.store();
    store.set_open_chat_id_i64(-100);
    store.set_open_chat_is_forum(true);
    store.set_open_chat_topics(vec![
        topic(1, "General", 30),
        topic(2, "Rust", 20),
        topic(3, "Go", 10),
    ]);
    TopicListWindow::new(app_context)
}

fn selected_name(window: &TopicListWindow) -> Option<String> {
    let index = window.topic_list_state.selected()?;
    window.visible_topics.get(index).map(|t| t.name.clone())
}

fn row_text(text: &Text<'_>) -> Vec<String> {
    text.lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect()
}

#[test]
fn moving_stops_at_the_ends() {
    let mut window = create_test_topic_list();
    window.update(Action::TopicListPrevious);
    assert_eq!(selected_name(&window).as_deref(), Some("General"));
    window.update(Action::TopicListNext);
    window.update(Action::TopicListNext);
    window.update(Action::TopicListNext);
    assert_eq!(selected_name(&window).as_deref(), Some("Go"));
}

#[test]
fn the_selection_follows_its_topic_when_topics_move() {
    let mut window = create_test_topic_list();
    window.update(Action::TopicListNext);
    assert_eq!(selected_name(&window).as_deref(), Some("Rust"));
    // A new message moves Go to the top
    let message = MessageEntry::plain(1, TdMessageSender::User(2), "hi", 0);
    let go = libtgrs::tdlib_rs::enums::MessageTopic::Forum(
        libtgrs::tdlib_rs::types::MessageTopicForum { forum_topic_id: 3 },
    );
    window
        .app_context
        .store()
        .add_open_chat_topic_message(Some(&go), &message);
    window.refresh();
    assert_eq!(window.topic_list_state.selected(), Some(2));
    assert_eq!(selected_name(&window).as_deref(), Some("Rust"));
}

#[test]
fn opening_a_topic_makes_it_the_open_one() {
    let mut window = create_test_topic_list();
    window.update(Action::TopicListNext);
    window.update(Action::TopicListOpen);
    let store = window.app_context.store();
    assert_eq!(store.open_topic_id(), Some(2));
    assert_eq!(store.open_topic().map(|t| t.name).as_deref(), Some("Rust"));

    // A newly opened forum starts again at its first topic, unless one is open
    store.set_open_topic_id(None);
    window.update(Action::LoadForumTopics);
    window.refresh();
    assert_eq!(selected_name(&window).as_deref(), Some("General"));
}

#[test]
fn rows_show_the_icon_name_and_badges() {
    let app_context = create_test_app_context();
    let mut entry = topic(2, "Rust", 0);
    entry.is_pinned = true;
    entry.is_closed = true;
    entry.unread_count = 4;

    let text = topic_list_item_text(&entry, &app_context, 30, false, false);
    let rows = row_text(&text);

    assert_eq!(text.lines.len(), usize::from(CHAT_LIST_ITEM_ROWS));
    assert!(rows[0].contains("💬 Rust 🔒"), "{rows:?}");
    assert!(rows[1].contains("No messages"), "{rows:?}");
    assert!(rows[1].ends_with("📌  4 "), "{rows:?}");
    assert!(
        row_text(&topic_list_item_text(
            &topic(1, "General", 0),
            &app_context,
            30,
            false,
            false
        ))[0]
            .contains("# General")
    );
}

#[test]
fn a_topic_s_draft_shows_unless_it_is_open() {
    let app_context = create_test_app_context();
    let mut entry = topic(3, "Rust", 1);
    entry.draft = "unsent".to_owned();
    let second_line =
        |open| row_text(&topic_list_item_text(&entry, &app_context, 50, false, open))[1].clone();
    assert!(second_line(false).contains("Draft: unsent"));
    assert!(!second_line(true).contains("Draft:"));
}
