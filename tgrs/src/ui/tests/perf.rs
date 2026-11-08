//! Frame-cost benchmarks on a synthetic store (no TDLib connection).
//! Run with `cargo test --release -p tgt perf -- --ignored --nocapture`.

use std::{sync::Arc, time::Instant};

use crossterm::event::{KeyCode, KeyModifiers};
use libtgt::{
    model::{MessageEntry, OrderedChat, TdMessageSender},
    tdlib_rs::{
        enums::{ChatAvailableReactions, ChatList, ChatType},
        types::{
            Chat, ChatAvailableReactionsAll, ChatNotificationSettings, ChatPosition,
            ChatTypePrivate,
        },
    },
};
use ratatui::{Terminal, backend::TestBackend};

use super::create_test_app_context;
use crate::{
    app::{
        action::{Action, Modifiers},
        context::AppContext,
    },
    ui::{Tui, component_name::ComponentName},
};

const CHATS: i64 = 300;
const MESSAGES: i64 = 500;
const OPEN_CHAT: i64 = 1;

fn chat(id: i64, title: &str) -> Chat {
    Chat {
        id,
        r#type: ChatType::Private(ChatTypePrivate { user_id: id }),
        title: title.to_owned(),
        photo: None,
        accent_color_id: 0,
        background_custom_emoji_id: 0,
        upgraded_gift_colors: None,
        profile_accent_color_id: 0,
        profile_background_custom_emoji_id: 0,
        permissions: Default::default(),
        last_message: None,
        positions: Vec::new(),
        chat_lists: Vec::new(),
        message_sender_id: None,
        block_list: None,
        has_protected_content: false,
        is_translatable: false,
        is_marked_as_unread: false,
        view_as_topics: false,
        has_scheduled_messages: false,
        has_welcome_messages: false,
        can_be_deleted_only_for_self: false,
        can_be_deleted_for_all_users: false,
        can_be_reported: false,
        default_disable_notification: false,
        unread_count: 0,
        last_read_inbox_message_id: 0,
        last_read_outbox_message_id: 0,
        unread_mention_count: 0,
        unread_reaction_count: 0,
        unread_poll_vote_count: 0,
        notification_settings: ChatNotificationSettings::default(),
        available_reactions: ChatAvailableReactions::All(ChatAvailableReactionsAll::default()),
        message_auto_delete_time: 0,
        emoji_status: None,
        background: None,
        theme: None,
        action_bar: None,
        business_bot_manage_bar: None,
        video_chat: Default::default(),
        pending_join_requests: None,
        reply_markup_message_id: 0,
        draft_message: None,
        client_data: String::new(),
    }
}

fn populate(app_context: &AppContext) {
    let store = app_context.store();
    for id in 1..=CHATS {
        store
            .chats()
            .insert(id, chat(id, &format!("Synthetic chat {id}")));
        store.chats_index().insert(OrderedChat {
            chat_id: id,
            position: ChatPosition {
                list: ChatList::Main,
                order: 10_000 - id,
                is_pinned: false,
                source: None,
            },
        });
    }
    store.set_open_chat_id_i64(OPEN_CHAT);
    let lorem = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod \
                 tempor incididunt ut labore et dolore magna aliqua. ";
    store
        .open_chat_messages()
        .insert_messages((1..=MESSAGES).map(|id| {
            let text = lorem.repeat(1 + (id % 5) as usize);
            let sender = TdMessageSender::User(1 + id % 3);
            MessageEntry::plain(id, sender, &text, 1_700_000_000 + id as i32 * 60)
        }));
}

fn frame_micros(f: impl FnMut()) -> f64 {
    let mut f = f;
    let rounds = 200;
    let start = Instant::now();
    for _ in 0..rounds {
        f();
    }
    start.elapsed().as_secs_f64() * 1e6 / f64::from(rounds)
}

#[test]
#[ignore = "benchmark"]
fn frame_cost() {
    let app_context = create_test_app_context();
    populate(&app_context);
    let mut tui = Tui::new(Arc::clone(&app_context));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    tui.register_action_handler(tx).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(200, 60)).unwrap();
    let mut pump = |tui: &mut Tui| {
        while let Ok(action) = rx.try_recv() {
            tui.update(action);
        }
    };
    tui.update(Action::ChatHistoryAppended);
    tui.update(Action::FocusComponent(ComponentName::Prompt));
    pump(&mut tui);

    let mut draw = |tui: &mut Tui| {
        terminal
            .draw(|frame| tui.draw(frame, frame.area()).unwrap())
            .unwrap();
    };
    draw(&mut tui);

    let idle = frame_micros(|| draw(&mut tui));
    let keystroke = frame_micros(|| {
        tui.update(Action::Key(
            KeyCode::Char('a'),
            Modifiers::from(KeyModifiers::NONE),
        ));
        pump(&mut tui);
        draw(&mut tui);
    });
    tui.update(Action::FocusComponent(ComponentName::ChatList));
    tui.update(Action::ChatListSearch);
    pump(&mut tui);
    let search = frame_micros(|| {
        tui.update(Action::Key(
            KeyCode::Char('s'),
            Modifiers::from(KeyModifiers::NONE),
        ));
        pump(&mut tui);
        draw(&mut tui);
        tui.update(Action::Key(
            KeyCode::Backspace,
            Modifiers::from(KeyModifiers::NONE),
        ));
        pump(&mut tui);
        draw(&mut tui);
    }) / 2.0;
    let new_message = frame_micros(|| {
        tui.update(Action::ChatHistoryAppended);
        pump(&mut tui);
        draw(&mut tui);
    });
    let store = Arc::clone(app_context.store());
    let snapshot = frame_micros(|| {
        std::hint::black_box(store.ordered_messages_snapshot());
    });
    let chat_list = frame_micros(|| {
        std::hint::black_box(store.chat_list());
    });
    let empty = Terminal::new(TestBackend::new(200, 60));
    let mut empty = empty.unwrap();
    let flush = frame_micros(|| {
        empty.draw(|_| {}).unwrap();
    });
    println!(
        "PERF snapshot={snapshot:.0}us store_chat_list={chat_list:.0}us empty_draw={flush:.0}us"
    );
    println!(
        "PERF idle_frame={idle:.0}us keystroke_frame={keystroke:.0}us \
         chat_search_key={search:.0}us chat_list_update={new_message:.0}us"
    );
}

/// Drawing is driven by changes: once settled, drawing again must neither
/// send actions nor ask for another frame, or the run loop never sleeps.
#[test]
fn a_steady_screen_is_quiet() {
    let app_context = create_test_app_context();
    populate(&app_context);
    let mut tui = Tui::new(Arc::clone(&app_context));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    tui.register_action_handler(tx).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(160, 50)).unwrap();
    let screens = [
        vec![Action::ChatHistoryAppended],
        vec![Action::FocusComponent(ComponentName::Prompt)],
        vec![Action::Paste("a draft\nover\nlines".to_owned())],
        vec![Action::FocusComponent(ComponentName::Chat)],
        vec![Action::FocusComponent(ComponentName::ChatList)],
        vec![Action::ChatListSearch],
        vec![Action::ShowCommandGuide],
    ];
    for actions in screens {
        for action in actions {
            tui.update(action.clone());
        }
        // Settle: what the actions and the first frames ask for
        for _ in 0..10 {
            while let Ok(action) = rx.try_recv() {
                tui.update(action);
            }
            while let Ok(action) = app_context.action_rx().try_recv() {
                tui.update(action);
            }
            app_context.clear_render_flag();
            terminal
                .draw(|frame| tui.draw(frame, frame.area()).unwrap())
                .unwrap();
        }
        app_context.clear_render_flag();
        terminal
            .draw(|frame| tui.draw(frame, frame.area()).unwrap())
            .unwrap();
        let sent: Vec<Action> = std::iter::from_fn(|| rx.try_recv().ok())
            .chain(std::iter::from_fn(|| {
                app_context.action_rx().try_recv().ok()
            }))
            .collect();
        assert!(sent.is_empty(), "a steady frame sent {sent:?}");
        assert!(
            !app_context.needs_render(),
            "a steady frame asked for another"
        );
    }
}
