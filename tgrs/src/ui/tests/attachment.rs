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

#[test]
fn copied_files_come_before_any_image() {
    // Dolphin offers the files, their text and an icon of the first one.
    let offered = ["text/uri-list", "text/plain", "image/png"];
    assert_eq!(
        clipboard_picks(&offered),
        [
            ClipboardPick::Files,
            ClipboardPick::Image("image/png", "png")
        ]
    );
}

#[test]
fn a_jpeg_xl_alone_is_pasted_as_one() {
    let offered = ["image/jxl", "text/plain"];
    assert_eq!(
        clipboard_picks(&offered),
        [ClipboardPick::Image("image/jxl", "jxl")]
    );
}

#[test]
fn images_tdlib_sends_as_they_are_are_preferred() {
    let offered = [
        "image/gif",
        "image/jxl",
        "image/webp",
        "IMAGE/JPEG",
        "image/png",
    ];
    let picks: Vec<_> = clipboard_picks(&offered)
        .into_iter()
        .map(|pick| match pick {
            ClipboardPick::Image(_, ext) => ext,
            ClipboardPick::Files => "files",
        })
        .collect();
    assert_eq!(picks, ["png", "jpg", "webp", "jxl", "gif"]);
}

#[test]
fn text_or_unknown_images_paste_nothing() {
    assert!(clipboard_picks(&["text/plain", "UTF8_STRING"]).is_empty());
    assert!(clipboard_picks(&["image/x-unknown"]).is_empty());
    assert!(clipboard_picks::<&str>(&[]).is_empty());
}

#[test]
fn a_uri_list_of_files_is_those_files() {
    let (_dir, a, spaced) = files();
    let uri = |p: &PathBuf| format!("file://{}", p.display().to_string().replace(' ', "%20"));
    let list = format!(
        "# copied by a file manager\r\n{}\r\n{}\r\n",
        uri(&a),
        uri(&spaced)
    );
    assert_eq!(uri_list_files(&list), Some(vec![a, spaced]));
}

#[test]
fn a_uri_list_with_a_link_or_a_missing_file_is_not_files() {
    let (dir, a, _) = files();
    let file = format!("file://{}", a.display());
    assert_eq!(
        uri_list_files(&format!("{file}\nhttps://example.org/a.jpg")),
        None
    );
    let missing = format!("file://{}", dir.path().join("gone.jxl").display());
    assert_eq!(uri_list_files(&format!("{file}\n{missing}")), None);
    assert_eq!(uri_list_files("# only a comment\n"), None);
    // A directory isn't a file to send.
    assert_eq!(
        uri_list_files(&format!("file://{}", dir.path().display())),
        None
    );
}
