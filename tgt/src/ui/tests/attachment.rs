use super::*;
use std::fs;

/// A directory with `a.jpg` and `my photo.png`.
fn files() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.jpg");
    let spaced = dir.path().join("my photo.png");
    fs::write(&a, b"x").unwrap();
    fs::write(&spaced, b"x").unwrap();
    (dir, a, spaced)
}

#[test]
fn a_dropped_file_is_its_path() {
    let (_dir, a, _) = files();
    let paste = a.display().to_string();
    assert_eq!(dropped_files(&paste), Some(vec![a.clone()]));
    // Terminals may add a space or a line break
    assert_eq!(dropped_files(&format!("{paste} \n")), Some(vec![a]));
}

#[test]
fn spaces_are_quoted_or_escaped() {
    let (_dir, a, spaced) = files();
    let quoted = format!("'{}'", spaced.display());
    assert_eq!(dropped_files(&quoted), Some(vec![spaced.clone()]));
    let double = format!("\"{}\"", spaced.display());
    assert_eq!(dropped_files(&double), Some(vec![spaced.clone()]));
    let escaped = spaced.display().to_string().replace(' ', "\\ ");
    assert_eq!(
        dropped_files(&format!("{escaped} {}", a.display())),
        Some(vec![spaced, a])
    );
}

#[test]
fn file_uris_are_decoded() {
    let (_dir, a, spaced) = files();
    let uri = |path: &PathBuf| format!("file://{}", path.display().to_string().replace(' ', "%20"));
    assert_eq!(
        dropped_files(&format!("{}\n{}", uri(&a), uri(&spaced))),
        Some(vec![a.clone(), spaced])
    );
    let localhost = format!("file://localhost{}", a.display());
    assert_eq!(dropped_files(&localhost), Some(vec![a]));
}

#[test]
fn anything_else_is_text() {
    let (dir, a, _) = files();
    for paste in [
        String::new(),
        "hello".to_owned(),
        "a.jpg".to_owned(),
        // A path in a sentence
        format!("see {}", a.display()),
        // A directory or a missing file
        dir.path().display().to_string(),
        dir.path().join("missing.txt").display().to_string(),
        format!("'{}", a.display()),
        "https://example.com/a.jpg".to_owned(),
    ] {
        assert_eq!(dropped_files(&paste), None, "{paste:?}");
    }
}

#[test]
fn names_are_the_file_names() {
    let paths = [PathBuf::from("/x/a.jpg"), PathBuf::from("/y/b c.pdf")];
    assert_eq!(names(&paths), "a.jpg, b c.pdf");
}
