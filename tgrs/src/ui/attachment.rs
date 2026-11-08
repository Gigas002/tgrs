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
    if let Some(rest) = word.strip_prefix("file://") {
        // file:///path or file://localhost/path
        let path = rest.strip_prefix("localhost").unwrap_or(rest);
        if !path.starts_with('/') {
            return None;
        }
        return percent_decode(path).map(PathBuf::from);
    }
    if let Some(rest) = word.strip_prefix("~/") {
        return std::env::var_os("HOME").map(|home| Path::new(&home).join(rest));
    }
    word.starts_with('/').then(|| PathBuf::from(word))
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

/// Saves the image on the clipboard as a PNG in the temporary directory, to
/// be sent; why not otherwise.
#[cfg(feature = "media-preview")]
pub fn save_clipboard_image() -> Result<PathBuf, String> {
    let image = arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.get_image())
        .map_err(|e| match e {
            arboard::Error::ContentNotAvailable => "No image on the clipboard.".to_owned(),
            e => format!("Can't read the clipboard: {e}"),
        })?;
    let width = u32::try_from(image.width).map_err(|e| e.to_string())?;
    let height = u32::try_from(image.height).map_err(|e| e.to_string())?;
    let buffer = image::RgbaImage::from_raw(width, height, image.bytes.into_owned())
        .ok_or("The clipboard image is malformed.")?;
    let dir = std::env::temp_dir().join("tgt-clipboard");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Can't save the image: {e}"))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let path = dir.join(format!("clipboard-{stamp}.png"));
    buffer
        .save_with_format(&path, image::ImageFormat::Png)
        .map_err(|e| format!("Can't save the image: {e}"))?;
    Ok(path)
}

/// Without image support, pasting images isn't available.
#[cfg(not(feature = "media-preview"))]
pub fn save_clipboard_image() -> Result<PathBuf, String> {
    Err("Pasting images needs tgt built with the media-preview feature.".to_owned())
}

#[cfg(test)]
#[path = "tests/attachment.rs"]
mod tests;
