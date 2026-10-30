use std::path::PathBuf;

use super::*;
use crate::{app::commands, settings::Dirs};

fn test_app_context() -> Arc<AppContext> {
    AppContext::for_tests()
}

fn temp_dirs(root: &std::path::Path) -> Dirs {
    Dirs {
        home: root.to_path_buf(),
        config: root.join("config"),
        data: root.join("data"),
        state: root.join("state"),
    }
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
fn init_config_writes_bundled_files_and_keeps_existing() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = temp_dirs(tmp.path());
    let config = dirs.config.join("config.toml");

    commands::init_config(&dirs, false).unwrap();
    assert!(config.exists());
    assert!(dirs.config.join("keymap.toml").exists());
    assert!(dirs.config.join("themes/theme.toml").exists());

    std::fs::write(&config, "# mine").unwrap();
    commands::init_config(&dirs, false).unwrap();
    assert_eq!(std::fs::read_to_string(&config).unwrap(), "# mine");

    commands::init_config(&dirs, true).unwrap();
    assert_ne!(std::fs::read_to_string(&config).unwrap(), "# mine");
}

#[test]
fn clear_removes_selected_directories() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = temp_dirs(tmp.path());
    for dir in [&dirs.config, &dirs.data, &dirs.state] {
        std::fs::create_dir_all(dir).unwrap();
    }

    commands::clear(&dirs, false, true, true, true).unwrap();
    assert!(dirs.config.exists());
    assert!(!dirs.data.exists());
    assert!(!dirs.state.exists());
}

#[test]
fn clear_requires_a_selection() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(commands::clear(&temp_dirs(tmp.path()), false, false, false, true).is_err());
}

#[test]
fn clear_refuses_project_directories() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = temp_dirs(tmp.path());
    std::fs::create_dir_all(&dirs.data).unwrap();
    std::fs::write(dirs.data.join("Cargo.toml"), "").unwrap();

    assert!(commands::clear(&dirs, false, true, false, true).is_err());
    assert!(dirs.data.exists());
}

#[test]
fn unsafe_to_remove_detects_root_and_repositories() {
    assert!(commands::is_unsafe_to_remove(&PathBuf::from("/")));
    let tmp = tempfile::tempdir().unwrap();
    assert!(!commands::is_unsafe_to_remove(tmp.path()));
    std::fs::create_dir(tmp.path().join(".git")).unwrap();
    assert!(commands::is_unsafe_to_remove(tmp.path()));
}

#[test]
fn confirm_accepts_yes_only() {
    assert!(commands::confirm("", "y\n".as_bytes()).unwrap());
    assert!(commands::confirm("", "YES\n".as_bytes()).unwrap());
    assert!(!commands::confirm("", "\n".as_bytes()).unwrap());
    assert!(!commands::confirm("", "no\n".as_bytes()).unwrap());
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
