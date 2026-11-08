//! Handing media to other programs: opening files with the desktop's default
//! application and saving them to the download directory.

use std::{
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[cfg(test)]
mod tests;

/// Opens files with the application associated with their type.
pub const OPENER: &str = "xdg-open";

/// Extensions of programs, scripts and installers, which the default
/// application may run instead of showing.
const EXECUTABLE_EXTENSIONS: &[&str] = &[
    "apk",
    "appimage",
    "bash",
    "bat",
    "bin",
    "cmd",
    "com",
    "csh",
    "deb",
    "desktop",
    "exe",
    "fish",
    "flatpak",
    "flatpakref",
    "jar",
    "js",
    "ksh",
    "lnk",
    "msi",
    "pl",
    "ps1",
    "py",
    "rb",
    "rpm",
    "run",
    "scr",
    "sh",
    "snap",
    "vbs",
    "zsh",
];

/// Whether opening `path` could run it as a program.
pub fn is_executable_type(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            EXECUTABLE_EXTENSIONS
                .iter()
                .any(|known| known.eq_ignore_ascii_case(ext))
        })
}

/// Schemes of links from messages that are opened. Others, like `file:` or
/// custom handlers, could open local files or start programs.
const URL_SCHEMES: &[&str] = &["http", "https", "mailto", "tel", "tg"];

/// What to open for a link in a message: bare domains get `https://`, e-mail
/// addresses `mailto:` and phone numbers `tel:`. `None` when the link has
/// another scheme or isn't a single address.
pub fn link_url(target: &str) -> Option<String> {
    let target = target.trim();
    if target.is_empty() || target.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return phone_url(target);
    }
    if let Some((scheme, _)) = target.split_once("://") {
        return URL_SCHEMES
            .iter()
            .any(|known| known.eq_ignore_ascii_case(scheme))
            .then(|| target.to_owned());
    }
    if let Some((scheme, rest)) = target.split_once(':')
        && !scheme.is_empty()
        && scheme.chars().all(|c| c.is_ascii_alphabetic())
    {
        // `scheme:...` without `//`, like `mailto:` or `javascript:`
        let known = ["mailto", "tel", "tg"]
            .iter()
            .any(|known| known.eq_ignore_ascii_case(scheme));
        return (known && !rest.is_empty()).then(|| target.to_owned());
    }
    if target.contains('@') && !target.contains('/') {
        return Some(format!("mailto:{target}"));
    }
    if let Some(phone) = phone_url(target) {
        return Some(phone);
    }
    // A bare domain; `-` first could be read as an option by the opener
    (!target.starts_with('-')).then(|| format!("https://{target}"))
}

/// `tel:` for phone numbers like `+1 (555) 010-99`.
fn phone_url(target: &str) -> Option<String> {
    let digits = target.chars().filter(char::is_ascii_digit).count();
    let only_phone_chars = target
        .chars()
        .all(|c| c.is_ascii_digit() || " +-().".contains(c));
    (digits >= 3 && only_phone_chars).then(|| {
        let number: String = target
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == '+')
            .collect();
        format!("tel:{number}")
    })
}

/// Opens `url` in the default browser (or mail/phone application).
pub fn open_url(url: &str) -> io::Result<()> {
    run_opener(url.as_ref())
}

/// Opens `path` with the default application (e.g. mpv for videos, an image
/// viewer for photos). Blocks until the opener hands the file over.
/// Programs and scripts are refused: received files must not run with a
/// single key press.
pub fn open_with_default_app(path: &Path) -> io::Result<()> {
    if is_executable_type(path) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "programs and scripts aren't opened, download the file instead",
        ));
    }
    run_opener(path.as_os_str())
}

/// Runs the opener on `target` and waits for it to hand it over.
fn run_opener(target: &OsStr) -> io::Result<()> {
    let status = Command::new(OPENER)
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| io::Error::new(e.kind(), format!("failed to run {OPENER}: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("{OPENER} exited with {status}")))
    }
}

/// Copies `source` into `dir` as `file_name`, without replacing existing
/// files: `name.ext` becomes `name (1).ext`, `name (2).ext`, ...
pub fn save_to_dir(source: &Path, dir: &Path, file_name: &str) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let mut input = fs::File::open(source)?;
    let file_name = sanitize_file_name(file_name);
    loop {
        let dest = unique_path(dir, &file_name);
        // `create_new` fails instead of replacing a file created meanwhile
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&dest)
        {
            Ok(mut output) => {
                if let Err(e) = io::copy(&mut input, &mut output) {
                    drop(output);
                    let _ = fs::remove_file(&dest);
                    return Err(e);
                }
                return Ok(dest);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
}

/// A file name that stays inside the target directory: path separators and
/// control characters are replaced, and leading dots and spaces are dropped.
pub fn sanitize_file_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| if c.is_control() || c == '/' { '_' } else { c })
        .collect();
    let sanitized = sanitized.trim_matches([' ', '.']);
    if sanitized.is_empty() {
        "file".to_owned()
    } else {
        sanitized.to_owned()
    }
}

/// `dir/file_name`, or the first `dir/stem (n).ext` that doesn't exist.
pub fn unique_path(dir: &Path, file_name: &str) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .map_or(file_name.into(), |stem| stem.to_string_lossy());
    let extension = path
        .extension()
        .map(|ext| format!(".{}", ext.to_string_lossy()))
        .unwrap_or_default();
    (1..)
        .map(|n| dir.join(format!("{stem} ({n}){extension}")))
        .find(|candidate| !candidate.exists())
        .expect("an unused file name exists")
}
