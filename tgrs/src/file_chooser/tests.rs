use super::*;

#[test]
fn file_uris_become_paths() {
    assert_eq!(
        file_uri_path("file:///home/me/a%20b.jpg"),
        Some(PathBuf::from("/home/me/a b.jpg"))
    );
    assert_eq!(
        file_uri_path("file://localhost/tmp/%D1%84.png"),
        Some(PathBuf::from("/tmp/ф.png"))
    );
    // A stray percent stays
    assert_eq!(
        file_uri_path("file:///tmp/100%.txt"),
        Some(PathBuf::from("/tmp/100%.txt"))
    );
}

#[test]
fn other_uris_are_not_local_files() {
    assert_eq!(file_uri_path("https://example.com/a.jpg"), None);
    assert_eq!(file_uri_path("file://server/share/a.jpg"), None);
    assert_eq!(file_uri_path("/tmp/a.jpg"), None);
    // Not UTF-8
    assert_eq!(file_uri_path("file:///tmp/%FF.jpg"), None);
}

#[test]
fn the_answer_follows_the_response_code() {
    let uris = vec![
        "file:///tmp/a.jpg".to_owned(),
        "sftp://host/b.jpg".to_owned(),
        "file:///tmp/c.pdf".to_owned(),
    ];
    assert_eq!(
        answer(0, &uris),
        Ok(Answer::Picked(vec![
            PathBuf::from("/tmp/a.jpg"),
            PathBuf::from("/tmp/c.pdf")
        ]))
    );
    assert_eq!(answer(1, &[]), Ok(Answer::Cancelled));
    assert_eq!(answer(2, &[]), Err(ChooserError::Failed));
}

#[test]
fn the_request_path_is_known_before_the_call() {
    assert_eq!(
        request_path(":1.42", "tgrs_7_9"),
        "/org/freedesktop/portal/desktop/request/1_42/tgrs_7_9"
    );
}

#[cfg(not(feature = "file-chooser"))]
#[test]
fn without_the_feature_there_is_no_dialog() {
    assert!(matches!(
        pick_files("Send files", &|| false),
        Err(ChooserError::Unavailable(_))
    ));
}

/// Shows the desktop's real file dialog: pick some files by hand, with
/// `cargo test -p tgrs picks_files_through_the_portal -- --ignored --nocapture`.
#[cfg(feature = "file-chooser")]
#[test]
#[ignore = "shows the desktop's file dialog"]
fn picks_files_through_the_portal() {
    let answer = pick_files("tgrs test: pick any files", &|| false);
    eprintln!("{answer:?}");
    assert!(answer.is_ok());
}
