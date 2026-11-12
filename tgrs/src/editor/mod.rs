//! Writing messages in an external editor (`app.editor`, `$EDITOR` or
//! `$VISUAL`), on a temporary file that only the user can read.

use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(test)]
mod tests;

/// Why the text couldn't be edited; it's left as it was.
#[derive(Debug, thiserror::Error)]
pub enum EditorError {
    #[error("no editor: set app.editor or $EDITOR")]
    NoEditor,
    #[error("can't write the text to edit: {0}")]
    TempFile(io::Error),
    #[error("can't run {program}: {source}")]
    Spawn { program: String, source: io::Error },
    #[error("{program} exited with {status}: the text is unchanged")]
    Failed { program: String, status: String },
    #[error("can't read the edited text: {0}")]
    Read(io::Error),
}

/// Opens `text` in the editor `command` (the program and its arguments; the
/// file comes last) on a file in `dir`, and returns the text once the
/// editor exits. The terminal must be free for the editor meanwhile. The
/// trailing newlines editors add are dropped.
pub fn edit_text(command: &[String], text: &str, dir: &Path) -> Result<String, EditorError> {
    let (program, args) = command.split_first().ok_or(EditorError::NoEditor)?;
    let file = TempFile::create(dir, text).map_err(EditorError::TempFile)?;
    let status = Command::new(program)
        .args(args)
        .arg(&file.path)
        .status()
        .map_err(|source| EditorError::Spawn {
            program: program.clone(),
            source,
        })?;
    if !status.success() {
        return Err(EditorError::Failed {
            program: program.clone(),
            status: status.to_string(),
        });
    }
    let edited = fs::read_to_string(&file.path).map_err(EditorError::Read)?;
    Ok(edited.trim_end_matches(['\n', '\r']).to_owned())
}

/// A file only the user can read, deleted when dropped.
struct TempFile {
    path: PathBuf,
}

impl TempFile {
    fn create(dir: &Path, text: &str) -> io::Result<Self> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let path = dir.join(format!("tgrs-message-{}-{nanos}.txt", std::process::id()));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        // Deleted again if it can't be written
        let temp = Self { path };
        file.write_all(text.as_bytes())?;
        Ok(temp)
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        if let Err(e) = fs::remove_file(&self.path) {
            tracing::warn!("can't delete {}: {e}", self.path.display());
        }
    }
}
