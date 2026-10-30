//! Background work on message media: downloading it from Telegram, opening it
//! with the default application and saving it to the download directory.
//! Progress and errors are reported in the status bar.

use std::{path::Path, sync::Arc};

use libtgt::{
    client::Files,
    model::{Media, MediaFile},
};

#[cfg(feature = "media-preview")]
use crate::app::action::{PreviewDecodedPayload, PreviewRequest};
use crate::{app::action::Action, app::context::AppContext, media};

/// Opens the media of a message with the default application.
pub fn open(app_context: &Arc<AppContext>, files: Files, message_id: i64) {
    let Some(media) = message_media(app_context, message_id) else {
        return;
    };
    let app_context = Arc::clone(app_context);
    tokio::spawn(async move {
        let name = media.suggested_file_name();
        let Some(path) = local_path(&app_context, files, &media.file, &name).await else {
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

/// Copies the media of a message into the download directory.
pub fn download(app_context: &Arc<AppContext>, files: Files, message_id: i64) {
    let Some(media) = message_media(app_context, message_id) else {
        return;
    };
    let app_context = Arc::clone(app_context);
    tokio::spawn(async move {
        let name = media.suggested_file_name();
        let Some(path) = local_path(&app_context, files, &media.file, &name).await else {
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

/// The file on disk, downloading it from Telegram first when needed.
async fn local_path(
    app_context: &AppContext,
    files: Files,
    file: &MediaFile,
    name: &str,
) -> Option<String> {
    if let Some(path) = file.downloaded_path() {
        return Some(path.to_owned());
    }
    status(app_context, format!("Downloading {name}…"));
    match files.download(file.id).await {
        Ok(path) if !path.is_empty() => Some(path),
        Ok(_) => {
            status(app_context, format!("Failed to download {name}"));
            None
        }
        Err(e) => {
            status(
                app_context,
                format!("Failed to download {name}: {}", e.message),
            );
            None
        }
    }
}

pub(super) fn status(app_context: &AppContext, message: String) {
    let _ = app_context.action_tx().send(Action::StatusMessage(message));
}
