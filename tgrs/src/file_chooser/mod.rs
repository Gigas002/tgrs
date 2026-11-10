//! Picking files to send in the desktop's own file dialog, through the XDG
//! desktop portal (`org.freedesktop.portal.FileChooser`) on the session
//! D-Bus: GNOME, KDE and wlroots desktops each show their native dialog.

use std::path::PathBuf;

#[cfg(test)]
mod tests;

/// Why the desktop's file dialog couldn't be shown.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ChooserError {
    /// No session bus or no portal implementing the file chooser.
    #[error("no file dialog: {0}")]
    Unavailable(String),
    /// The portal showed the dialog but failed.
    #[error("the file dialog failed")]
    Failed,
}

/// What the portal answered (`org.freedesktop.portal.Request.Response`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// Files were picked; the others are skipped, e.g. remote URIs.
    Picked(Vec<PathBuf>),
    /// The dialog was closed without picking anything.
    Cancelled,
}

/// The portal's answer from its response code (0 picked, 1 cancelled,
/// 2 failed) and the `uris` it returned. Only local files are kept.
pub fn answer(response: u32, uris: &[String]) -> Result<Answer, ChooserError> {
    match response {
        0 => Ok(Answer::Picked(
            uris.iter().filter_map(|uri| file_uri_path(uri)).collect(),
        )),
        1 => Ok(Answer::Cancelled),
        _ => Err(ChooserError::Failed),
    }
}

/// A local `file://` URI as a path, e.g. `file:///home/me/a%20b.jpg`.
pub fn file_uri_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let path = rest.strip_prefix("localhost").unwrap_or(rest);
    if !path.starts_with('/') {
        return None;
    }
    percent_decode(path).map(PathBuf::from)
}

/// Decodes `%xx` escapes; None if they don't make UTF-8.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(byte) = text
                .get(i + 1..i + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            decoded.push(byte);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

/// The object path of the request the portal makes for a call from the bus
/// name `sender` (e.g. `:1.42`) with `token`, known before the call so that
/// its answer can't be missed.
pub fn request_path(sender: &str, token: &str) -> String {
    let sender = sender.trim_start_matches(':').replace('.', "_");
    format!("/org/freedesktop/portal/desktop/request/{sender}/{token}")
}

#[cfg(feature = "file-chooser")]
mod portal {
    use std::{
        sync::{Arc, Mutex},
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use dbus::{
        arg::{PropMap, RefArg, Variant},
        blocking::Connection,
        message::MatchRule,
    };

    use super::{Answer, ChooserError, answer, request_path};

    const DESTINATION: &str = "org.freedesktop.portal.Desktop";
    const OBJECT: &str = "/org/freedesktop/portal/desktop";
    const FILE_CHOOSER: &str = "org.freedesktop.portal.FileChooser";
    const REQUEST: &str = "org.freedesktop.portal.Request";
    /// How long the portal has to show the dialog; picking takes as long as
    /// it takes.
    const CALL_TIMEOUT: Duration = Duration::from_secs(30);
    /// How often `stop` is checked while the dialog is open.
    const POLL: Duration = Duration::from_millis(200);

    fn unavailable(e: dbus::Error) -> ChooserError {
        ChooserError::Unavailable(e.message().unwrap_or("D-Bus error").to_owned())
    }

    /// Shows the dialog titled `title` to pick files, several at once, and
    /// waits for the answer. Gives up, closing the dialog, once `stop`
    /// says so.
    pub fn pick_files(title: &str, stop: &dyn Fn() -> bool) -> Result<Answer, ChooserError> {
        let connection = Connection::new_session().map_err(unavailable)?;
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let token = format!("tgrs_{}_{nanos}", std::process::id());
        let expected = request_path(&connection.unique_name(), &token);

        // The response code and the picked URIs
        type Response = Arc<Mutex<Option<(u32, Vec<String>)>>>;
        let response: Response = Arc::default();
        let listen = |path: String| {
            let response = Arc::clone(&response);
            connection.add_match(
                MatchRule::new_signal(REQUEST, "Response").with_path(path),
                move |(code, results): (u32, PropMap), _, _| {
                    let uris = results
                        .get("uris")
                        .and_then(|uris| uris.0.as_iter())
                        .map(|uris| {
                            uris.filter_map(|uri| uri.as_str().map(str::to_owned))
                                .collect()
                        })
                        .unwrap_or_default();
                    if let Ok(mut response) = response.lock() {
                        *response = Some((code, uris));
                    }
                    false
                },
            )
        };
        listen(expected.clone()).map_err(unavailable)?;

        let mut options = PropMap::new();
        options.insert("handle_token".into(), Variant(Box::new(token)));
        options.insert("multiple".into(), Variant(Box::new(true)));
        options.insert("modal".into(), Variant(Box::new(true)));
        let proxy = connection.with_proxy(DESTINATION, OBJECT, CALL_TIMEOUT);
        let (handle,): (dbus::Path,) = proxy
            .method_call(FILE_CHOOSER, "OpenFile", ("", title, options))
            .map_err(unavailable)?;
        // Portals older than version 0.9 ignore the token
        if *handle != *expected {
            listen(handle.to_string()).map_err(unavailable)?;
        }

        loop {
            let answered = response.lock().ok().and_then(|mut r| r.take());
            if let Some((code, uris)) = answered {
                return answer(code, &uris);
            }
            if stop() {
                let request = connection.with_proxy(DESTINATION, handle.clone(), CALL_TIMEOUT);
                let _: Result<(), _> = request.method_call(REQUEST, "Close", ());
                return Ok(Answer::Cancelled);
            }
            connection.process(POLL).map_err(|e| {
                tracing::error!("file dialog: {e}");
                ChooserError::Failed
            })?;
        }
    }
}

/// Shows the desktop's file dialog titled `title` to pick files to send,
/// several at once, and waits for the answer. Gives up, closing the dialog,
/// once `stop` says so.
#[cfg(feature = "file-chooser")]
pub fn pick_files(title: &str, stop: &dyn Fn() -> bool) -> Result<Answer, ChooserError> {
    portal::pick_files(title, stop)
}

/// Without the `file-chooser` feature there's no file dialog.
#[cfg(not(feature = "file-chooser"))]
pub fn pick_files(_title: &str, _stop: &dyn Fn() -> bool) -> Result<Answer, ChooserError> {
    Err(ChooserError::Unavailable(
        "tgrs was built without the file-chooser feature".to_owned(),
    ))
}
