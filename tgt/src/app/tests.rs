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

    assert_eq!(ctx.store().jump_target_message_id_i64(), 42);
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
