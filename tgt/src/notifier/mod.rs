//! Desktop notifications for new messages (the `notifications` feature).

use libtgt::model::MessageAlert;

#[cfg(test)]
mod tests;

/// Application name shown by the notification server.
pub const APP_NAME: &str = "tgt";
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

/// Shows a desktop notification in the background; failures are only logged.
#[cfg(feature = "notifications")]
pub fn show(alert: MessageAlert, show_preview: bool) {
    let summary = alert.title();
    let body = body(&alert, show_preview);
    #[cfg(all(unix, not(target_os = "macos")))]
    let body = escape_markup(&body);
    // Talking to the notification server blocks
    tokio::task::spawn_blocking(move || {
        if let Err(e) = notify_rust::Notification::new()
            .appname(APP_NAME)
            .summary(&summary)
            .body(&body)
            .show()
        {
            tracing::warn!("failed to show a notification: {e}");
        }
    });
}

#[cfg(not(feature = "notifications"))]
pub fn show(alert: MessageAlert, _show_preview: bool) {
    tracing::debug!(
        chat_id = alert.chat_id,
        "built without the notifications feature"
    );
}
