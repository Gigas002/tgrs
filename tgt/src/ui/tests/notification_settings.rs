use super::*;
use crate::ui::tests::create_test_app_context;
use libtgt::model::{ChatSettings, NotificationScope, ScopeSettings};
use ratatui::{Terminal, backend::TestBackend};
use tokio::sync::mpsc;

fn popup() -> (NotificationSettingsPopup, mpsc::UnboundedReceiver<Action>) {
    let mut popup = NotificationSettingsPopup::new(create_test_app_context());
    let (tx, rx) = mpsc::unbounded_channel();
    popup.register_action_handler(tx).unwrap();
    popup.update(Action::ShowNotificationSettings);
    (popup, rx)
}

fn scope(scope: NotificationScope, alerts: bool) -> ScopeSettings {
    ScopeSettings {
        scope,
        alerts,
        preview: true,
        mentions: true,
        pinned_messages: false,
    }
}

/// Muted channels, and a private chat with its preview hidden.
fn loaded(with_chat: bool) -> Action {
    Action::NotificationSettingsLoaded(NotificationSettings {
        scopes: vec![
            scope(NotificationScope::PrivateChats, true),
            scope(NotificationScope::Groups, true),
            scope(NotificationScope::Channels, false),
        ],
        chat: with_chat.then(|| ChatSettings {
            chat_id: 7,
            title: "Ferris".into(),
            scope: NotificationScope::PrivateChats,
            alerts: None,
            preview: Some(false),
            mentions: None,
            pinned_messages: None,
        }),
    })
}

fn sent(rx: &mut mpsc::UnboundedReceiver<Action>) -> Vec<Action> {
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

fn screen(popup: &mut NotificationSettingsPopup) -> String {
    let (width, height) = (100, 24);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| popup.draw(frame, frame.area()).unwrap())
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .chunks(usize::from(width))
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_open_chat_comes_first_then_each_scope() {
    let (mut popup, _rx) = popup();
    assert!(screen(&mut popup).contains("Loading…"));
    popup.update(loaded(true));
    assert_eq!(
        popup.shown(),
        [
            "This chat · Ferris",
            "Notifications: default (on)",
            "Message preview: off",
            "Private chats",
            "Notifications: on",
            "Message preview: on",
            "Groups",
            "Notifications: on",
            "Message preview: on",
            "Mentions and replies: on",
            "Pinned messages: off",
            "Channels",
            "Notifications: off",
            "Message preview: on",
            "Pinned messages: off",
        ]
    );
    let screen = screen(&mut popup);
    assert!(
        screen.contains("Notifications         default (on)"),
        "{screen}"
    );
}

#[test]
fn without_an_open_chat_only_scopes_show() {
    let (mut popup, _rx) = popup();
    popup.update(loaded(false));
    assert_eq!(popup.shown()[0], "Private chats");
}

#[test]
fn moving_skips_the_headers() {
    let (mut popup, mut rx) = popup();
    popup.update(loaded(false));
    // Private chats' preview, then Groups' notifications
    popup.update(Action::NotificationSettingsNext);
    popup.update(Action::NotificationSettingsNext);
    popup.update(Action::NotificationSettingsToggle);
    assert_eq!(
        sent(&mut rx),
        [Action::SetNotificationOption(
            NotificationTarget::Scope(NotificationScope::Groups),
            NotificationOption::Alerts,
            Some(false)
        )]
    );
    assert_eq!(popup.shown()[4], "Notifications: off");
    // Back over the header, wrapping around from the top
    popup.update(Action::NotificationSettingsPrevious);
    popup.update(Action::NotificationSettingsPrevious);
    popup.update(Action::NotificationSettingsPrevious);
    popup.update(Action::NotificationSettingsToggle);
    assert_eq!(
        sent(&mut rx),
        [Action::SetNotificationOption(
            NotificationTarget::Scope(NotificationScope::Channels),
            NotificationOption::PinnedMessages,
            Some(true)
        )]
    );
}

#[test]
fn a_chat_option_cycles_through_its_default() {
    let (mut popup, mut rx) = popup();
    popup.update(loaded(true));
    let alerts = |value| {
        Action::SetNotificationOption(
            NotificationTarget::Chat(7),
            NotificationOption::Alerts,
            value,
        )
    };
    popup.update(Action::NotificationSettingsToggle);
    assert_eq!(popup.shown()[1], "Notifications: off");
    popup.update(Action::NotificationSettingsToggle);
    assert_eq!(popup.shown()[1], "Notifications: on");
    popup.update(Action::NotificationSettingsToggle);
    assert_eq!(popup.shown()[1], "Notifications: default (on)");
    assert_eq!(
        sent(&mut rx),
        [alerts(Some(false)), alerts(Some(true)), alerts(None)]
    );
}

#[test]
fn a_chat_default_follows_its_scope() {
    let (mut popup, _rx) = popup();
    popup.update(loaded(true));
    // Private chats' notifications
    for _ in 0..2 {
        popup.update(Action::NotificationSettingsNext);
    }
    popup.update(Action::NotificationSettingsToggle);
    assert_eq!(popup.shown()[1], "Notifications: default (off)");
}

#[test]
fn reloading_keeps_the_highlighted_option() {
    let (mut popup, mut rx) = popup();
    popup.update(loaded(false));
    popup.update(Action::NotificationSettingsNext);
    popup.update(loaded(false));
    popup.update(Action::NotificationSettingsToggle);
    assert_eq!(
        sent(&mut rx),
        [Action::SetNotificationOption(
            NotificationTarget::Scope(NotificationScope::PrivateChats),
            NotificationOption::Preview,
            Some(false)
        )]
    );
}

#[test]
fn settings_for_a_closed_popup_are_dropped() {
    let (mut popup, _rx) = popup();
    popup.update(Action::HideNotificationSettings);
    popup.update(loaded(true));
    assert!(popup.shown().is_empty());
}
