//! Background work on message media: downloading it from Telegram, opening it
//! with the default application and saving it to the download directory.
//! Progress and errors are reported in the status bar.

use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use libtgrs::{
    client::Files,
    model::{Media, TransferPurpose, TransferState},
};

#[cfg(feature = "media-preview")]
use crate::app::action::{PreviewDecodedPayload, PreviewRequest};
use crate::{
    app::action::Action,
    app::context::AppContext,
    file_chooser::{self, Answer, ChooserError},
    media,
};

/// Opens the media of a message with the default application.
pub fn open(app_context: &Arc<AppContext>, files: Files, message_id: i64) {
    if let Some(media) = message_media(app_context, message_id) {
        fetch(app_context, files, media, TransferPurpose::Open);
    }
}

/// Copies the media of a message into the download directory.
pub fn download(app_context: &Arc<AppContext>, files: Files, message_id: i64) {
    if let Some(media) = message_media(app_context, message_id) {
        fetch(app_context, files, media, TransferPurpose::Save);
    }
}

/// Downloads `media` if needed, followed in the downloads popup, then opens
/// or saves it. Also resumes a paused download.
pub fn fetch(app_context: &Arc<AppContext>, files: Files, media: Media, purpose: TransferPurpose) {
    match purpose {
        TransferPurpose::Open => open_media(app_context, files, media),
        TransferPurpose::Save => save_media(app_context, files, media),
    }
}

fn open_media(app_context: &Arc<AppContext>, files: Files, media: Media) {
    let app_context = Arc::clone(app_context);
    tokio::spawn(async move {
        let name = media.suggested_file_name();
        let Some(path) = local_path(&app_context, files, &media, TransferPurpose::Open).await
        else {
            return;
        };
        status(&app_context, format!("Opening {name}…"));
        let result =
            tokio::task::spawn_blocking(move || media::open_with_default_app(Path::new(&path)))
                .await;
        match result {
            Ok(Ok(())) => status(&app_context, format!("Opened {name}")),
            Ok(Err(e)) => status(&app_context, format!("Failed to open {name}: {e}")),
            Err(e) => tracing::error!("open task failed: {e}"),
        }
    });
}

/// Opens a link from a message in the default browser; links with unknown
/// schemes are refused.
pub fn open_url(app_context: &Arc<AppContext>, target: &str) {
    let Some(url) = media::link_url(target) else {
        status(app_context, format!("Not opened, not a web link: {target}"));
        return;
    };
    let app_context = Arc::clone(app_context);
    tokio::spawn(async move {
        let shown = url.clone();
        match tokio::task::spawn_blocking(move || media::open_url(&url)).await {
            Ok(Ok(())) => status(&app_context, format!("Opened {shown}")),
            Ok(Err(e)) => status(&app_context, format!("Failed to open {shown}: {e}")),
            Err(e) => tracing::error!("open URL task failed: {e}"),
        }
    });
}

fn save_media(app_context: &Arc<AppContext>, files: Files, media: Media) {
    let app_context = Arc::clone(app_context);
    tokio::spawn(async move {
        let name = media.suggested_file_name();
        let Some(path) = local_path(&app_context, files, &media, TransferPurpose::Save).await
        else {
            return;
        };
        let dir = app_context.app_config().download_dir.clone();
        let file_name = name.clone();
        let result = tokio::task::spawn_blocking(move || {
            media::save_to_dir(Path::new(&path), &dir, &file_name)
        })
        .await;
        match result {
            Ok(Ok(dest)) => status(&app_context, format!("Saved to {}", dest.display())),
            Ok(Err(e)) => status(&app_context, format!("Failed to save {name}: {e}")),
            Err(e) => tracing::error!("download task failed: {e}"),
        }
    });
}

/// Largest side, in pixels, of previews shown in the chat. They take a few
/// rows, so a small image is enough and keeps the cache small.
#[cfg(feature = "media-preview")]
const CHAT_PREVIEW_MAX_DIMENSION: u32 = 400;

/// Downloads (unless the request has a path) and decodes a preview image, then
/// sends [`Action::PreviewDecoded`]. Large images are downscaled to
/// 400 pixels on their longer side.
#[cfg(feature = "media-preview")]
pub fn load_preview(app_context: &Arc<AppContext>, files: Files, request: PreviewRequest) {
    let action_tx = app_context.action_tx().clone();
    tokio::spawn(async move {
        let path = match request.path.clone() {
            Some(path) => Ok(path),
            None => files.download(request.file_id).await.map_err(|e| e.message),
        };
        let result = match path {
            Ok(path) => {
                tokio::task::spawn_blocking(move || decode(&path, CHAT_PREVIEW_MAX_DIMENSION))
                    .await
                    .unwrap_or_else(|e| Err(e.to_string()))
            }
            Err(e) => Err(e),
        };
        let payload = PreviewDecodedPayload { request, result };
        let _ = action_tx.send(Action::PreviewDecoded(payload));
    });
}

#[cfg(feature = "media-preview")]
fn decode(path: &str, max_dimension: u32) -> Result<image::DynamicImage, String> {
    let image = image::open(path).map_err(|e| e.to_string())?;
    if image.width().max(image.height()) <= max_dimension {
        return Ok(image);
    }
    // `thumbnail` keeps the aspect ratio within the bounds
    Ok(image.thumbnail(max_dimension, max_dimension))
}

fn message_media(app_context: &AppContext, message_id: i64) -> Option<Media> {
    let media = app_context
        .store()
        .get_message(message_id)
        .and_then(|message| message.media().cloned());
    if media.is_none() {
        status(app_context, "This message has no media.".into());
    }
    media
}

/// The file on disk, downloading it from Telegram first when needed. The
/// download is followed in the downloads popup; one paused or cancelled
/// there ends quietly.
async fn local_path(
    app_context: &AppContext,
    files: Files,
    media: &Media,
    purpose: TransferPurpose,
) -> Option<String> {
    let file = &media.file;
    if let Some(path) = file.downloaded_path() {
        return Some(path.to_owned());
    }
    let name = media.suggested_file_name();
    let store = app_context.store();
    store.start_transfer(media, purpose);
    status(
        app_context,
        format!("Downloading {name}… (space t: downloads)"),
    );
    let result = files.download(file.id).await;
    // Paused or cancelled meanwhile: not a failure
    let stopped = store
        .transfer(file.id)
        .is_some_and(|t| matches!(t.state, TransferState::Paused | TransferState::Cancelled));
    match result {
        _ if stopped => None,
        Ok(path) if !path.is_empty() => {
            store.set_transfer_state(file.id, TransferState::Done);
            Some(path)
        }
        Ok(_) => {
            let why = "no file".to_owned();
            store.set_transfer_state(file.id, TransferState::Failed(why));
            status(app_context, format!("Failed to download {name}"));
            None
        }
        Err(e) => {
            store.set_transfer_state(file.id, TransferState::Failed(e.message.clone()));
            status(
                app_context,
                format!("Failed to download {name}: {}", e.message),
            );
            None
        }
    }
}

/// The desktop's file dialog: whether it's open, and whether it failed to
/// show, after which the built-in file explorer is used.
#[derive(Debug, Default)]
pub struct FileDialog {
    open: AtomicBool,
    unavailable: AtomicBool,
}

impl FileDialog {
    /// Whether the dialog can be shown: it didn't fail to before.
    pub fn is_available(&self) -> bool {
        !self.unavailable.load(Ordering::Acquire)
    }
}

/// Shows the desktop's file dialog to pick files to send to the open chat;
/// the prompt then sends them. Without a dialog, the built-in file explorer
/// is shown instead, now and from then on.
pub fn pick_files(app_context: &Arc<AppContext>, dialog: &Arc<FileDialog>) {
    let store = app_context.store();
    let chat_id = store.open_chat_id().as_i64();
    if chat_id == 0 {
        status(app_context, "Open a chat before sending files.".to_owned());
        return;
    }
    if dialog.open.swap(true, Ordering::AcqRel) {
        status(app_context, "The file dialog is open already.".to_owned());
        return;
    }
    let title = match store.name_of_open_chat_id() {
        Some(name) => format!("Send files to {name}"),
        None => "Send files".to_owned(),
    };
    let app_context = Arc::clone(app_context);
    let dialog = Arc::clone(dialog);
    tokio::spawn(async move {
        let ctx = Arc::clone(&app_context);
        let picked = tokio::task::spawn_blocking(move || {
            // Quitting closes the dialog
            file_chooser::pick_files(&title, &|| ctx.quit_acquire())
        })
        .await;
        dialog.open.store(false, Ordering::Release);
        let action_tx = app_context.action_tx();
        match picked {
            Ok(Ok(Answer::Picked(paths))) if paths.is_empty() => {
                status(&app_context, "Only local files can be sent.".to_owned());
            }
            Ok(Ok(Answer::Picked(paths))) => {
                // Not to another chat opened while picking
                if app_context.store().open_chat_id().as_i64() == chat_id {
                    let paths = paths.iter().map(|p| p.display().to_string()).collect();
                    let _ = action_tx.send(Action::UploadFiles(paths));
                } else {
                    status(
                        &app_context,
                        "Not sent: another chat was opened meanwhile.".to_owned(),
                    );
                }
            }
            Ok(Ok(Answer::Cancelled)) => {}
            Ok(Err(e @ ChooserError::Unavailable(_))) => {
                tracing::warn!("{e}");
                dialog.unavailable.store(true, Ordering::Release);
                status(
                    &app_context,
                    "No file dialog on this desktop: using the built-in explorer".to_owned(),
                );
                let _ = action_tx.send(Action::ShowFileUploadExplorer);
            }
            Ok(Err(e)) => status(&app_context, format!("Can't pick files: {e}")),
            Err(e) => tracing::error!("file dialog task failed: {e}"),
        }
        app_context.mark_dirty();
    });
}

pub(super) fn status(app_context: &AppContext, message: String) {
    let _ = app_context.action_tx().send(Action::StatusMessage(message));
}
