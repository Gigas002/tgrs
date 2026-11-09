//! Helpers shared by the component tests in this directory.

use std::sync::Arc;

use libtgrs::model::{ChatListEntry, Media, MediaFile, MediaKind, MessageEntry, TdMessageSender};

use crate::{app::context::AppContext, ui::component_traits::Component};

pub fn create_test_app_context() -> Arc<AppContext> {
    AppContext::for_tests()
}

/// A plain text message from user 1.
pub fn create_mock_message(id: i64, content: &str) -> MessageEntry {
    MessageEntry::plain(id, TdMessageSender::User(1), content, 0)
}

/// A document message from user 1.
pub fn create_mock_media_message(id: i64, file_name: &str) -> MessageEntry {
    let media = Media {
        kind: MediaKind::Document,
        file: MediaFile {
            id: 7,
            size: 2048,
            ..MediaFile::default()
        },
        file_name: file_name.to_owned(),
        mime_type: "application/zip".to_owned(),
        duration_secs: 0,
        preview: None,
        thumbnail: None,
    };
    MessageEntry::with_media(id, TdMessageSender::User(1), media, 0)
}

/// A photo message from user 1, with a smaller size for the chat preview.
#[cfg(feature = "media-preview")]
pub fn create_mock_photo_message(id: i64, thumbnail_id: i32) -> MessageEntry {
    let file = |id| MediaFile {
        id,
        ..MediaFile::default()
    };
    let media = Media {
        kind: MediaKind::Photo,
        file: file(8),
        file_name: String::new(),
        mime_type: "image/jpeg".to_owned(),
        duration_secs: 0,
        preview: Some(file(8)),
        thumbnail: Some(file(thumbnail_id)),
    };
    MessageEntry::with_media(id, TdMessageSender::User(1), media, 0)
}

pub fn create_mock_chat(chat_id: i64, chat_name: &str) -> ChatListEntry {
    ChatListEntry {
        chat_id,
        chat_name: chat_name.to_owned(),
        ..ChatListEntry::default()
    }
}

/// The whole screen: header, panes and status bar, like concord's.
#[test]
fn the_screen_has_a_header_panes_and_hints() {
    use ratatui::{Terminal, backend::TestBackend};
    let app_context = create_test_app_context();
    let mut tui = super::Tui::new(Arc::clone(&app_context));
    let (width, height) = (100, 30);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| tui.draw(frame, frame.area()).unwrap())
        .unwrap();
    let rows: Vec<String> = terminal
        .backend()
        .buffer()
        .content()
        .chunks(usize::from(width))
        .map(|row| row.iter().map(|cell| cell.symbol()).collect())
        .collect();
    let screen = rows.join("\n");
    assert!(rows[0].starts_with(" tgrs - v"), "{screen}");
    assert!(
        rows[0].contains("Loading..."),
        "not logged in yet:\n{screen}"
    );
    assert!(rows[1].contains("┌ Chats "), "{screen}");
    assert!(rows[1].contains("┌ Chat "), "{screen}");
    assert!(screen.contains("Message"), "composer title:\n{screen}");
    assert!(
        screen.contains("Press i or Alt+3 to write a message"),
        "{screen}"
    );
    let last = rows.last().unwrap();
    assert!(last.contains("Space menu"), "{screen}");
    assert!(last.contains("Alt+F1 help"), "{screen}");
    assert!(last.contains("q quit"), "{screen}");
}

mod perf;

/// A left click at `column`, `row`.
pub fn click(column: u16, row: u16) -> crossterm::event::MouseEvent {
    mouse(
        crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column,
        row,
    )
}

/// The mouse wheel turned down (`true`) or up.
pub fn wheel(down: bool) -> crossterm::event::MouseEvent {
    use crossterm::event::MouseEventKind;
    let kind = if down {
        MouseEventKind::ScrollDown
    } else {
        MouseEventKind::ScrollUp
    };
    mouse(kind, 0, 0)
}

fn mouse(
    kind: crossterm::event::MouseEventKind,
    column: u16,
    row: u16,
) -> crossterm::event::MouseEvent {
    crossterm::event::MouseEvent {
        kind,
        column,
        row,
        modifiers: crossterm::event::KeyModifiers::NONE,
    }
}

/// Draws `component` on a `width` x `height` screen.
pub fn draw_component(component: &mut dyn Component, width: u16, height: u16) {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| component.draw(frame, frame.area()).unwrap())
        .unwrap();
}
