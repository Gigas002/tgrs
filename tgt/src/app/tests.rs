use super::*;

fn test_app_context() -> Arc<AppContext> {
    AppContext::for_tests()
}

#[test]
fn fold_compresses_long_cha_refresh_run() {
    let v = vec![
        Action::ChatHistoryAppended,
        Action::ChatHistoryAppended,
        Action::Refresh,
        Action::ChatHistoryAppended,
    ];
    let f = fold_chat_list_refresh_actions(v);
    assert_eq!(f.len(), 2);
    assert!(matches!(f[0], Action::Refresh));
    assert!(matches!(f[1], Action::ChatHistoryAppended));
}

#[test]
fn fold_preserves_other_actions_in_order() {
    let v = vec![
        Action::ChatHistoryAppended,
        Action::Quit,
        Action::Refresh,
        Action::ChatHistoryAppended,
    ];
    let f = fold_chat_list_refresh_actions(v);
    assert_eq!(f.len(), 4);
    assert!(matches!(f[0], Action::ChatHistoryAppended));
    assert!(matches!(f[1], Action::Quit));
    assert!(matches!(f[2], Action::Refresh));
    assert!(matches!(f[3], Action::ChatHistoryAppended));
}

#[test]
fn needs_render_returns_false_initially() {
    let ctx = test_app_context();
    assert!(!ctx.needs_render());
}

#[test]
fn set_needs_render_sets_flag() {
    let ctx = test_app_context();
    ctx.set_needs_render(true);
    assert!(ctx.needs_render());
    ctx.set_needs_render(false);
    assert!(!ctx.needs_render());
}

#[test]
fn mark_dirty_sets_needs_render() {
    let ctx = test_app_context();
    ctx.mark_dirty();
    assert!(ctx.needs_render());
}

#[test]
fn mark_dirty_idempotent() {
    let ctx = test_app_context();
    ctx.mark_dirty();
    ctx.mark_dirty();
    assert!(ctx.needs_render());
}

#[test]
fn should_render_matches_needs_render() {
    let ctx = test_app_context();
    assert!(!ctx.should_render());
    ctx.mark_dirty();
    assert!(ctx.should_render());
}

#[test]
fn clear_render_flag_returns_previous_and_clears() {
    let ctx = test_app_context();
    assert!(!ctx.clear_render_flag());
    ctx.mark_dirty();
    assert!(ctx.clear_render_flag());
    assert!(!ctx.needs_render());
}

#[test]
fn notifications_map_to_actions() {
    let ctx = test_app_context();
    handle_notification(&ctx, Notification::OpenChatMessageAdded(42)).unwrap();
    handle_notification(&ctx, Notification::OpenChatPinsChanged).unwrap();
    handle_notification(&ctx, Notification::OpenChatHistoryInvalidated).unwrap();
    handle_notification(&ctx, Notification::ChatListChanged).unwrap();

    assert_eq!(ctx.store().jump_target_message_id_i64(), -1);
    let mut rx = ctx.action_rx();
    let actions: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    assert_eq!(
        actions,
        vec![
            Action::ChatHistoryAppended,
            Action::ViewAllMessages,
            Action::LoadPinnedMessages,
            Action::GetChatHistory,
            Action::ChatHistoryAppended,
        ]
    );
}

#[test]
fn a_new_sign_in_is_told_in_the_status_bar() {
    let ctx = test_app_context();
    let alert = libtgt::model::SignInAlert {
        device_model: "Pixel 8".into(),
        location: "Berlin, Germany".into(),
    };
    handle_notification(&ctx, Notification::NewSignIn(alert)).unwrap();
    let mut rx = ctx.action_rx();
    assert_eq!(
        rx.try_recv().ok(),
        Some(Action::StatusMessage(
            "New sign-in on Pixel 8 from Berlin, Germany. Not you? Review sessions with space d."
                .into()
        ))
    );
}

#[test]
fn forwarding_tells_where_and_how_many() {
    use crate::app::action::ForwardRequest;
    use libtgt::tdlib_rs::types::Error;
    let request = |count: usize| ForwardRequest {
        to_chat_id: 2,
        to_chat_name: "Ada".into(),
        from_chat_id: 1,
        message_ids: (1..=count as i64).collect(),
        send_copy: false,
    };
    assert_eq!(forward_status(&request(1), 1, Ok(0)), "Forwarded to Ada");
    assert_eq!(
        forward_status(&request(3), 3, Ok(0)),
        "Forwarded 3 messages to Ada"
    );
    assert_eq!(
        forward_status(&request(3), 3, Ok(1)),
        "Forwarded 2 of 3 messages to Ada"
    );
    assert_eq!(
        forward_status(&request(1), 1, Ok(1)),
        "Can't forward to Ada"
    );
    let error = Error {
        code: 400,
        message: "CHAT_WRITE_FORBIDDEN".into(),
    };
    assert_eq!(
        forward_status(&request(1), 1, Err(error)),
        "Can't forward to Ada: CHAT_WRITE_FORBIDDEN"
    );
}

mod masked_input {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use crate::app::auth::read_masked;

    fn keys(text: &str) -> Vec<KeyEvent> {
        text.chars()
            .map(|c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
            .collect()
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    #[test]
    fn echoes_a_star_per_character() {
        let mut input = keys("pässwörd");
        input.push(key(KeyCode::Enter));
        let mut out = Vec::new();
        assert_eq!(read_masked(input, &mut out).as_deref(), Some("pässwörd"));
        assert_eq!(String::from_utf8(out).unwrap(), "********\r\n");
    }

    #[test]
    fn backspace_and_ctrl_u_edit_the_secret() {
        let mut input = keys("abc");
        input.push(key(KeyCode::Backspace));
        input.push(ctrl('u'));
        input.push(key(KeyCode::Backspace));
        input.extend(keys("xy"));
        input.push(key(KeyCode::Enter));
        input.extend(keys("ignored"));
        let mut out = Vec::new();
        assert_eq!(read_masked(input, &mut out).as_deref(), Some("xy"));
        let out = String::from_utf8(out).unwrap();
        assert!(!out.contains(['a', 'b', 'c', 'x', 'y']), "{out:?}");
    }

    #[test]
    fn ctrl_c_cancels() {
        let mut input = keys("secret");
        input.push(ctrl('c'));
        assert_eq!(read_masked(input, &mut Vec::new()), None);
    }
}

#[test]
fn changed_messages_are_redrawn() {
    let ctx = test_app_context();
    ctx.clear_render_flag();
    handle_notification(&ctx, Notification::OpenChatMessageChanged(1)).unwrap();
    assert!(ctx.needs_render());
}

#[test]
fn the_reaction_picker_lists_our_reactions_first() {
    use libtgt::model::{MessageEntry, Reaction, TdMessageSender};
    let ctx = test_app_context();
    let reaction = |emoji: &str, chosen| Reaction {
        emoji: emoji.into(),
        count: 1,
        chosen,
    };
    let message = MessageEntry::plain(3, TdMessageSender::User(1), "hi", 0).with_reactions(vec![
        reaction("🔥", false),
        reaction("🎉", true),
        reaction(Reaction::CUSTOM, true),
    ]);
    ctx.store().open_chat_messages().insert_messages([message]);

    let picker = reaction_picker(&ctx, 3, vec!["👍".into(), "🎉".into()]);
    let choice = |emoji: &str, chosen| ReactionChoice {
        emoji: emoji.into(),
        chosen,
    };
    // Custom emoji can't be sent back by their symbol, so they're left out
    assert_eq!(
        picker,
        Action::ShowPicker(PickerKind::Reactions {
            message_id: 3,
            reactions: vec![choice("🎉", true), choice("👍", false)],
        })
    );
}

#[test]
fn shifted_letters_match_their_bindings() {
    use crate::app::event::{Event, normalize_key};
    use crossterm::event::{KeyCode, KeyModifiers};
    // Terminals report `R` as Shift+R; keymaps name it `R` or `shift+r`
    let pressed = normalize_key(KeyCode::Char('R'), KeyModifiers::SHIFT);
    assert_eq!(pressed, (KeyCode::Char('R'), KeyModifiers::NONE));
    let pressed = Event::Key(pressed.0, pressed.1);
    assert_eq!("R".parse::<Event>().unwrap(), pressed);
    assert_eq!("shift+r".parse::<Event>().unwrap(), pressed);
    // Other keys keep Shift
    assert_eq!(
        normalize_key(KeyCode::Enter, KeyModifiers::SHIFT),
        (KeyCode::Enter, KeyModifiers::SHIFT)
    );
    assert_eq!(
        normalize_key(
            KeyCode::Char('x'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT
        ),
        (KeyCode::Char('X'), KeyModifiers::CONTROL)
    );

    let keymap = test_app_context().keymap_config().clone();
    let reply = keymap
        .get_map_of(Some(crate::ui::component_name::ComponentName::Chat))
        .get(&pressed);
    assert!(
        matches!(
            reply,
            Some(ActionBinding::Single {
                action: Action::SelectedMessage(crate::app::action::MessageCommand::Reply),
                ..
            })
        ),
        "{reply:?}"
    );
}

#[test]
fn leader_keys_close_the_menu_then_run() {
    use crate::app::event::Event;
    use crossterm::event::{KeyCode, KeyModifiers};
    let ctx = test_app_context();
    let sent = |ctx: &AppContext| {
        let mut rx = ctx.action_rx();
        std::iter::from_fn(|| rx.try_recv().ok()).collect::<Vec<_>>()
    };
    handle_leader_key(&ctx, &Event::Key(KeyCode::Char('1'), KeyModifiers::NONE)).unwrap();
    assert_eq!(
        sent(&ctx),
        vec![Action::HideLeaderMenu, Action::ToggleChatList]
    );
    // Unbound keys (and Esc) only close it
    handle_leader_key(&ctx, &Event::Key(KeyCode::Esc, KeyModifiers::NONE)).unwrap();
    assert_eq!(sent(&ctx), vec![Action::HideLeaderMenu]);
}

#[test]
fn space_opens_the_leader_menu_except_where_text_is_typed() {
    use crate::app::event::Event;
    use crate::ui::component_name::ComponentName;
    let ctx = test_app_context();
    let space: Event = "space".parse().unwrap();
    assert_eq!(space.to_string(), "Space");
    let binding = |component| match ctx.keymap_config().get_map_of(Some(component)).get(&space) {
        Some(ActionBinding::Single { action, .. }) => Some(action.clone()),
        _ => None,
    };
    for component in [ComponentName::Chat, ComponentName::ChatList] {
        assert_eq!(
            binding(component),
            Some(Action::ShowLeaderMenu),
            "{component}"
        );
    }
    for component in [ComponentName::Prompt, ComponentName::SearchOverlay] {
        assert_eq!(binding(component), None, "{component}");
    }
}

#[test]
fn shifted_symbols_like_bang_match_their_bindings() {
    use crate::app::action::{ChatCommand, MessageCommand};
    use crate::app::event::{Event, normalize_key};
    use crate::ui::component_name::ComponentName;
    use crossterm::event::{KeyCode, KeyModifiers};
    // Terminals report `!` as Shift+!
    let (code, modifiers) = normalize_key(KeyCode::Char('!'), KeyModifiers::SHIFT);
    let pressed = Event::Key(code, modifiers);
    assert_eq!("!".parse::<Event>().unwrap(), pressed);

    let keymap = test_app_context().keymap_config().clone();
    let bound = |component| match keymap.get_map_of(Some(component)).get(&pressed) {
        Some(ActionBinding::Single { action, .. }) => Some(action.clone()),
        _ => None,
    };
    assert_eq!(
        bound(ComponentName::ChatList),
        Some(Action::SelectedChat(ChatCommand::Report))
    );
    assert_eq!(
        bound(ComponentName::Chat),
        Some(Action::SelectedMessage(MessageCommand::Report))
    );
}

#[test]
fn shift_k_and_shift_up_move_pinned_chats() {
    use crate::app::event::{Event, normalize_key};
    use crate::ui::component_name::ComponentName;
    use crossterm::event::{KeyCode, KeyModifiers};
    let keymap = test_app_context().keymap_config().clone();
    let bound = |component, code, modifiers| {
        let (code, modifiers) = normalize_key(code, modifiers);
        match keymap
            .get_map_of(Some(component))
            .get(&Event::Key(code, modifiers))
        {
            Some(ActionBinding::Single { action, .. }) => Some(action.clone()),
            _ => None,
        }
    };
    for list in [ComponentName::ChatList, ComponentName::ArchiveList] {
        let up = Some(Action::MovePinnedChat(true));
        let down = Some(Action::MovePinnedChat(false));
        assert_eq!(bound(list, KeyCode::Char('K'), KeyModifiers::SHIFT), up);
        assert_eq!(bound(list, KeyCode::Up, KeyModifiers::SHIFT), up);
        assert_eq!(bound(list, KeyCode::Char('J'), KeyModifiers::SHIFT), down);
        assert_eq!(bound(list, KeyCode::Down, KeyModifiers::SHIFT), down);
    }
}

#[test]
fn typing_is_told_every_few_seconds_per_chat() {
    let app_context = test_app_context();
    let now = std::time::Instant::now();
    assert!(app_context.should_send_typing(1, None, now));
    assert!(!app_context.should_send_typing(1, None, now + Duration::from_secs(1)));
    // Another chat or topic right away
    assert!(app_context.should_send_typing(2, None, now + Duration::from_secs(1)));
    assert!(app_context.should_send_typing(2, Some(5), now + Duration::from_secs(1)));
    assert!(app_context.should_send_typing(2, Some(5), now + context::TYPING_INTERVAL * 2));
    // Sending a message ends it, so the next key tells again
    app_context.reset_typing();
    assert!(app_context.should_send_typing(2, Some(5), now + context::TYPING_INTERVAL * 2));
}
