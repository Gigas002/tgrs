use std::os::unix::fs::PermissionsExt;

use super::*;

/// A fake editor: a shell script running `body` with the file as `$1`.
fn editor(dir: &Path, body: &str) -> Vec<String> {
    let path = dir.join("editor.sh");
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    vec![path.display().to_string()]
}

fn temp_files(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("tgrs-message-"))
        })
        .collect()
}

#[test]
fn the_edited_text_comes_back_without_trailing_newlines() {
    let dir = tempfile::tempdir().unwrap();
    let command = editor(dir.path(), r#"printf 'hello\nworld\n\n' > "$1""#);
    let text = edit_text(&command, "old", dir.path()).unwrap();
    assert_eq!(text, "hello\nworld");
}

#[test]
fn the_editor_gets_the_text_in_a_private_file_deleted_after() {
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen");
    let command = editor(
        dir.path(),
        &format!(
            r#"stat -c %a "$1" > {seen}; cat "$1" >> {seen}; echo ' and more' >> "$1""#,
            seen = seen.display()
        ),
    );
    let text = edit_text(&command, "привет 👋", dir.path()).unwrap();
    assert_eq!(text, "привет 👋 and more");
    assert_eq!(fs::read_to_string(&seen).unwrap(), "600\nпривет 👋");
    assert!(temp_files(dir.path()).is_empty());
}

#[test]
fn arguments_come_before_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut command = editor(dir.path(), r#"printf '%s|' "$@" > "$2""#);
    command.push("--wait".into());
    let text = edit_text(&command, "", dir.path()).unwrap();
    assert!(text.starts_with("--wait|"), "{text}");
    assert!(text.contains("tgrs-message-"), "{text}");
}

#[test]
fn a_failed_editor_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let command = editor(dir.path(), r#"echo changed > "$1"; exit 3"#);
    let err = edit_text(&command, "old", dir.path()).unwrap_err();
    assert!(matches!(err, EditorError::Failed { .. }), "{err}");
    assert!(err.to_string().contains("unchanged"), "{err}");
    assert!(temp_files(dir.path()).is_empty());
}

#[test]
fn a_missing_editor_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let command = vec!["/nonexistent/editor".to_owned()];
    let err = edit_text(&command, "old", dir.path()).unwrap_err();
    assert!(
        err.to_string().starts_with("can't run /nonexistent/editor"),
        "{err}"
    );
    assert!(temp_files(dir.path()).is_empty());
    assert!(matches!(
        edit_text(&[], "old", dir.path()),
        Err(EditorError::NoEditor)
    ));
}

#[test]
fn an_unwritable_directory_is_reported() {
    let err = edit_text(&["true".to_owned()], "old", Path::new("/nonexistent")).unwrap_err();
    assert!(matches!(err, EditorError::TempFile(_)), "{err}");
}
