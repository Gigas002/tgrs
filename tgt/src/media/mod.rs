//! Handing media to other programs: opening files with the desktop's default
//! application and saving them to the download directory.

use std::{
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
    let status = Command::new(OPENER)
        .arg(path)
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
