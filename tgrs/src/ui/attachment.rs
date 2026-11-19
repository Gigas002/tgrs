//! Files attached to the prompt: dropped on the terminal, which pastes their
//! paths, or an image pasted from the clipboard.

use std::path::{Path, PathBuf};

/// The files in a paste that is nothing but paths of existing files, as
/// terminals paste them when files are dropped on the window: separated by
/// spaces or lines, quoted or with escaped spaces, or as `file://` URIs.
/// None for any other paste, which is typed as text.
pub fn dropped_files(paste: &str) -> Option<Vec<PathBuf>> {
    let paths: Vec<PathBuf> = split_words(paste.trim())?
        .iter()
        .map(|word| to_path(word))
        .collect::<Option<_>>()?;
    let all_files = !paths.is_empty() && paths.iter().all(|path| path.is_file());
    all_files.then_some(paths)
}

/// Splits like a shell: on unquoted whitespace, with `'…'`, `"…"` and `\x`.
/// None for an unterminated quote.
fn split_words(text: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut quote = None;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"'), '\\') => word.push(chars.next()?),
            (Some(_), c) => word.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                in_word = true;
            }
            (None, '\\') => {
                word.push(chars.next()?);
                in_word = true;
            }
            (None, c) if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            (None, c) => {
                word.push(c);
                in_word = true;
            }
        }
    }
    if quote.is_some() {
        return None;
    }
    if in_word {
        words.push(word);
    }
    Some(words)
}

/// An absolute path, a `~/` one or a local `file://` URI as a path.
fn to_path(word: &str) -> Option<PathBuf> {
    if word.starts_with("file://") {
        return crate::file_chooser::file_uri_path(word);
    }
    if let Some(rest) = word.strip_prefix("~/") {
        return std::env::var_os("HOME").map(|home| Path::new(&home).join(rest));
    }
    word.starts_with('/').then(|| PathBuf::from(word))
}

/// The names of `paths` for the prompt's title, e.g. `a.jpg, b.pdf`.
pub fn names(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| {
            path.file_name().map_or_else(
                || path.display().to_string(),
                |n| n.to_string_lossy().into(),
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The clipboard's image types tgrs takes, in order of preference, with the
/// extension of the saved file. Formats TDLib sends as they are come first;
/// a JPEG XL is converted when sent as a photo.
const CLIPBOARD_IMAGES: [(&str, &str); 5] = [
    ("image/png", "png"),
    ("image/jpeg", "jpg"),
    ("image/webp", "webp"),
    ("image/jxl", "jxl"),
    ("image/gif", "gif"),
];

/// Copied files, as file managers put them on the clipboard.
const URI_LIST: &str = "text/uri-list";

/// What to paste from a clipboard that offers the `offered` types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardPick {
    /// Copied files: `text/uri-list`.
    Files,
    /// An image of this MIME type, saved with this extension.
    Image(&'static str, &'static str),
}

/// The types to try, best first: copied files, then an image. A file
/// manager may offer an icon or a thumbnail next to the files, so files win.
pub fn clipboard_picks<S: AsRef<str>>(offered: &[S]) -> Vec<ClipboardPick> {
    let offers = |mime: &str| {
        offered
            .iter()
            .any(|o| o.as_ref().eq_ignore_ascii_case(mime))
    };
    let files = offers(URI_LIST).then_some(ClipboardPick::Files);
    let images = CLIPBOARD_IMAGES
        .iter()
        .filter(|(mime, _)| offers(mime))
        .map(|&(mime, ext)| ClipboardPick::Image(mime, ext));
    files.into_iter().chain(images).collect()
}

/// The local files in a `text/uri-list`: one URI a line, `#` comments. None
/// unless every URI is an existing local file: a copied link isn't a file.
pub fn uri_list_files(list: &str) -> Option<Vec<PathBuf>> {
    let paths: Vec<PathBuf> = list
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(crate::file_chooser::file_uri_path)
        .collect::<Option<_>>()?;
    let all_files = !paths.is_empty() && paths.iter().all(|path| path.is_file());
    all_files.then_some(paths)
}

const NOTHING_TO_PASTE: &str = "No image or file on the clipboard.";

/// The files to attach from the clipboard: copied files as they are, or the
/// image on it saved in the temporary directory, in the format it was
/// copied in. Why not otherwise.
pub fn clipboard_attachments() -> Result<Vec<PathBuf>, String> {
    #[cfg(all(
        unix,
        not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))
    ))]
    if let Some(found) = wayland::attachments() {
        return found;
    }
    fallback_attachments()
}

/// Saves `bytes` of a pasted image as `clipboard-<time>.<ext>` in the
/// temporary directory.
fn save_pasted(bytes: &[u8], ext: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("tgrs-clipboard");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Can't save the image: {e}"))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let path = dir.join(format!("clipboard-{stamp}.{ext}"));
    std::fs::write(&path, bytes).map_err(|e| format!("Can't save the image: {e}"))?;
    Ok(path)
}

/// The data-control protocol of wlroots, KDE and GNOME compositors, which
/// lists every type the clipboard offers.
#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))
))]
mod wayland {
    use std::io::Read;
    use std::path::PathBuf;

    use wl_clipboard_rs::paste::{self, ClipboardType, Error, MimeType, Seat};

    use super::{ClipboardPick, NOTHING_TO_PASTE, clipboard_picks, save_pasted, uri_list_files};

    /// None when there's no Wayland clipboard to ask (an X11 session, or a
    /// compositor without the protocol), so the fallback is tried.
    pub fn attachments() -> Option<Result<Vec<PathBuf>, String>> {
        let offered = match paste::get_mime_types_ordered(ClipboardType::Regular, Seat::Unspecified)
        {
            Ok(offered) => offered,
            Err(Error::ClipboardEmpty | Error::NoSeats) => {
                return Some(Err(NOTHING_TO_PASTE.to_owned()));
            }
            Err(e) => {
                tracing::debug!("No Wayland clipboard: {e}");
                return None;
            }
        };
        for pick in clipboard_picks(&offered) {
            let mime = match pick {
                ClipboardPick::Files => super::URI_LIST,
                ClipboardPick::Image(mime, _) => mime,
            };
            let bytes = match read(mime) {
                Ok(bytes) => bytes,
                Err(e) => return Some(Err(format!("Can't read the clipboard: {e}"))),
            };
            match pick {
                ClipboardPick::Files => {
                    if let Some(files) = uri_list_files(&String::from_utf8_lossy(&bytes)) {
                        return Some(Ok(files));
                    }
                }
                ClipboardPick::Image(_, ext) => {
                    return Some(save_pasted(&bytes, ext).map(|path| vec![path]));
                }
            }
        }
        Some(Err(NOTHING_TO_PASTE.to_owned()))
    }

    fn read(mime: &str) -> Result<Vec<u8>, String> {
        let (mut pipe, _) = paste::get_contents(
            ClipboardType::Regular,
            Seat::Unspecified,
            MimeType::Specific(mime),
        )
        .map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        pipe.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        Ok(bytes)
    }
}

/// Through arboard, which reads copied files, and images as PNG only: X11,
/// and Wayland compositors without the data-control protocol.
fn fallback_attachments() -> Result<Vec<PathBuf>, String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("Can't read the clipboard: {e}"))?;
    if let Ok(files) = clipboard.get().file_list()
        && !files.is_empty()
        && files.iter().all(|path| path.is_file())
    {
        return Ok(files);
    }
    fallback_image(&mut clipboard).map(|path| vec![path])
}

#[cfg(feature = "media-preview")]
fn fallback_image(clipboard: &mut arboard::Clipboard) -> Result<PathBuf, String> {
    let image = clipboard.get_image().map_err(|e| match e {
        arboard::Error::ContentNotAvailable => NOTHING_TO_PASTE.to_owned(),
        e => format!("Can't read the clipboard: {e}"),
    })?;
    let width = u32::try_from(image.width).map_err(|e| e.to_string())?;
    let height = u32::try_from(image.height).map_err(|e| e.to_string())?;
    let buffer = image::RgbaImage::from_raw(width, height, image.bytes.into_owned())
        .ok_or("The clipboard image is malformed.")?;
    let mut png = std::io::Cursor::new(Vec::new());
    buffer
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|e| format!("Can't save the image: {e}"))?;
    save_pasted(png.get_ref(), "png")
}

/// arboard hands images over decoded; without an encoder they can't be saved.
#[cfg(not(feature = "media-preview"))]
fn fallback_image(_: &mut arboard::Clipboard) -> Result<PathBuf, String> {
    Err("Pasting images here needs tgrs built with the media-preview feature.".to_owned())
}

#[cfg(test)]
#[path = "tests/attachment.rs"]
mod tests;
