//! Desktop notifications for new messages (the `notifications` feature).

use crate::settings::Notifications;
use libtgrs::model::MessageAlert;

#[cfg(test)]
mod tests;

/// Application name shown by the notification server.
pub const APP_NAME: &str = "tgrs";
/// Longer message texts are cut to this many characters.
const MAX_BODY_CHARS: usize = 200;

/// The notification body: the message text, or a placeholder when it is hidden.
pub fn body(alert: &MessageAlert, show_preview: bool) -> String {
    match alert.text.as_deref().filter(|_| show_preview) {
        Some(text) if text.chars().count() > MAX_BODY_CHARS => {
            let cut: String = text.chars().take(MAX_BODY_CHARS).collect();
            format!("{}…", cut.trim_end())
        }
        Some(text) => text.to_owned(),
        None => "New message".to_owned(),
    }
}

/// Freedesktop notification servers may render a subset of HTML in the body.
pub fn escape_markup(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Whether `now` falls in the quiet hours, when nothing is shown.
pub fn is_quiet(settings: &Notifications, now: chrono::NaiveTime) -> bool {
    use chrono::Timelike;
    let minute = u16::try_from(now.hour() * 60 + now.minute()).unwrap_or(0);
    settings
        .quiet_hours
        .is_some_and(|hours| hours.contains(minute))
}

/// The sound to play for `alert`: none for silent messages.
pub fn sound<'a>(alert: &MessageAlert, settings: &'a Notifications) -> Option<&'a str> {
    settings.sound.as_deref().filter(|_| !alert.is_silent)
}

/// The terminal's title with `unread` messages, e.g. `tgrs (3)`.
pub fn window_title(unread: i32) -> String {
    if unread > 0 {
        format!("{APP_NAME} ({unread})")
    } else {
        APP_NAME.to_owned()
    }
}

/// Notifications waiting for a click at most; later ones aren't clickable,
/// so a flood of messages doesn't hold a thread each.
#[cfg(all(feature = "notifications", unix, not(target_os = "macos")))]
const MAX_CLICKABLE: usize = 8;
#[cfg(all(feature = "notifications", unix, not(target_os = "macos")))]
static CLICKABLE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Shows a desktop notification in the background; failures are only
/// logged. `open` is called with the chat when it's clicked.
#[cfg(feature = "notifications")]
pub fn show(
    alert: MessageAlert,
    settings: &Notifications,
    open: impl FnOnce(i64) + Send + 'static,
) {
    let summary = alert.title();
    let body = body(&alert, settings.show_preview);
    #[cfg(all(unix, not(target_os = "macos")))]
    let body = escape_markup(&body);
    let sound = sound(&alert, settings).map(str::to_owned);
    let chat_id = alert.chat_id;
    let open_on_click = settings.open_on_click;
    // Talking to the notification server blocks
    tokio::task::spawn_blocking(move || {
        let mut notification = notify_rust::Notification::new();
        notification.appname(APP_NAME).summary(&summary).body(&body);
        if let Some(sound) = &sound {
            notification.sound_name(sound);
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            use std::sync::atomic::Ordering;
            let clickable =
                open_on_click && CLICKABLE.fetch_add(1, Ordering::AcqRel) < MAX_CLICKABLE;
            if clickable {
                notification.action("default", "Open");
            } else if open_on_click {
                CLICKABLE.fetch_sub(1, Ordering::AcqRel);
            }
            match notification.show() {
                Ok(handle) if clickable => {
                    handle.wait_for_action(|action| {
                        if action == "default" {
                            open(chat_id);
                        }
                    });
                }
                Ok(_) => {}
                Err(e) => tracing::warn!("failed to show a notification: {e}"),
            }
            if clickable {
                CLICKABLE.fetch_sub(1, Ordering::AcqRel);
            }
        }
        #[cfg(not(all(unix, not(target_os = "macos"))))]
        {
            let _ = (open, open_on_click, chat_id);
            if let Err(e) = notification.show() {
                tracing::warn!("failed to show a notification: {e}");
            }
        }
    });
}

#[cfg(not(feature = "notifications"))]
pub fn show(
    alert: MessageAlert,
    _settings: &Notifications,
    _open: impl FnOnce(i64) + Send + 'static,
) {
    tracing::debug!(
        chat_id = alert.chat_id,
        "built without the notifications feature"
    );
}
