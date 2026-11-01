use super::*;
use crate::ui::tests::create_test_app_context;
use libtgt::model::OrderedChat;
use libtgt::tdlib_rs::types::ChatPosition;

/// Puts the chats in the archive, the first on top.
fn archive(app_context: &AppContext, chat_ids: &[i64]) {
    let mut index = app_context.store().archive_index();
    for (order, chat_id) in chat_ids.iter().rev().enumerate() {
        index.insert(OrderedChat {
            chat_id: *chat_id,
            position: ChatPosition {
                list: ChatList::Archive,
                order: order as i64 + 1,
                is_pinned: false,
                source: None,
            },
        });
    }
}

fn create_test_archive_list() -> ArchiveListWindow {
    let app_context = create_test_app_context();
    archive(&app_context, &[10, 20, 30]);
    ArchiveListWindow::new(app_context)
}

fn selected_id(window: &ArchiveListWindow) -> Option<i64> {
    let index = window.chat_list_state.selected()?;
    window.visible_chats.get(index).map(|c| c.chat_id)
}

fn sent_actions(app_context: &AppContext) -> Vec<Action> {
    let mut rx = app_context.action_rx();
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

#[test]
fn moving_stops_at_the_ends_and_loads_more_chats() {
    let mut window = create_test_archive_list();
    window.update(Action::ArchiveListPrevious);
    assert_eq!(selected_id(&window), Some(10));
    for _ in 0..5 {
        window.update(Action::ArchiveListNext);
    }
    assert_eq!(selected_id(&window), Some(30));
    assert!(
        sent_actions(&window.app_context)
            .contains(&Action::LoadChats(ChatList::Archive.into(), LOAD_MORE))
    );
}

#[test]
fn it_starts_at_the_open_chat() {
    let mut window = create_test_archive_list();
    window.update(Action::ArchiveListNext);
    window.app_context.store().set_open_chat_id_i64(30);
    // Shown again
    window.update(Action::FocusComponent(ComponentName::ArchiveList));
    window.refresh();
    assert_eq!(selected_id(&window), Some(30));
}

#[test]
fn opening_a_chat_makes_it_the_open_one() {
    let mut window = create_test_archive_list();
    window.update(Action::ArchiveListNext);
    window.update(Action::ArchiveListOpen);
    assert_eq!(window.app_context.store().open_chat_id().as_i64(), 20);
    let actions = sent_actions(&window.app_context);
    assert!(actions.contains(&Action::GetChatHistory), "{actions:?}");
    assert!(
        actions.contains(&Action::FocusComponent(ComponentName::Prompt)),
        "{actions:?}"
    );
    // Staying in the archive
    assert!(!actions.contains(&Action::CloseArchiveList), "{actions:?}");
}

#[test]
fn rows_are_padded_like_their_left_and_bottom() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut window = create_test_archive_list();
    let (width, height) = (30, 12);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| window.draw(frame, frame.area()).unwrap())
        .unwrap();
    let rows: Vec<String> = terminal
        .backend()
        .buffer()
        .content()
        .chunks(usize::from(width))
        .map(|row| row.iter().map(|cell| cell.symbol()).collect())
        .collect();
    let screen = rows.join("\n");
    assert!(rows[0].contains("Archive"), "{screen}");
    // A blank row under the top border, then the selected chat
    assert_eq!(rows[1].trim_matches(['│', ' ']), "", "{screen}");
    assert!(rows[2].starts_with("│▸ "), "{screen}");
    // Its time column ends as far from the right border as the marker is from the left
    let area = window.last_list_area.unwrap();
    assert_eq!((area.x, area.y), (1, 2));
    assert_eq!(area.width, width - 2 - 2);
    // Clicks map to rows below the padding
    window.focus();
    let click = |row| crossterm::event::MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 3,
        row,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    assert_eq!(window.handle_mouse_events(click(1)).unwrap(), None);
    assert_eq!(
        window.handle_mouse_events(click(5)).unwrap(),
        Some(Action::ArchiveListOpen)
    );
    assert_eq!(selected_id(&window), Some(20));
}

#[test]
fn a_pinned_archived_chat_moves_in_the_archive() {
    let app_context = create_test_app_context();
    {
        let mut index = app_context.store().archive_index();
        for (chat_id, order) in [(10, 3), (20, 2)] {
            index.insert(OrderedChat {
                chat_id,
                position: ChatPosition {
                    list: ChatList::Archive,
                    order,
                    is_pinned: true,
                    source: None,
                },
            });
        }
    }
    let mut window = ArchiveListWindow::new(app_context);
    window.update(Action::ArchiveListNext);
    assert_eq!(selected_id(&window), Some(20));
    window.update(Action::MovePinnedChat(true));
    assert!(
        sent_actions(&window.app_context).contains(&Action::SetPinnedChats(true, vec![20, 10]))
    );
}
