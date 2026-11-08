use super::*;

#[test]
fn sanitize_keeps_names_inside_the_directory() {
    assert_eq!(sanitize_file_name("report.pdf"), "report.pdf");
    assert_eq!(sanitize_file_name("../../etc/passwd"), "_.._etc_passwd");
    assert_eq!(sanitize_file_name("a\nb\tc.txt"), "a_b_c.txt");
    assert_eq!(sanitize_file_name(" .hidden "), "hidden");
    assert_eq!(sanitize_file_name(".."), "file");
    assert_eq!(sanitize_file_name(""), "file");
}

#[test]
fn unique_path_numbers_existing_names() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        unique_path(dir.path(), "a.tar.gz"),
        dir.path().join("a.tar.gz")
    );

    std::fs::write(dir.path().join("a.tar.gz"), "").unwrap();
    assert_eq!(
        unique_path(dir.path(), "a.tar.gz"),
        dir.path().join("a.tar (1).gz")
    );

    std::fs::write(dir.path().join("README"), "").unwrap();
    std::fs::write(dir.path().join("README (1)"), "").unwrap();
    assert_eq!(
        unique_path(dir.path(), "README"),
        dir.path().join("README (2)")
    );
}

#[test]
fn save_to_dir_creates_the_directory_and_never_overwrites() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("cache.bin");
    std::fs::write(&source, "data").unwrap();
    let downloads = tmp.path().join("Downloads");

    let first = save_to_dir(&source, &downloads, "archive.zip").unwrap();
    assert_eq!(first, downloads.join("archive.zip"));
    let second = save_to_dir(&source, &downloads, "archive.zip").unwrap();
    assert_eq!(second, downloads.join("archive (1).zip"));
    assert_eq!(std::fs::read_to_string(second).unwrap(), "data");
}

#[test]
fn programs_and_scripts_are_not_opened() {
    for name in [
        "setup.sh",
        "app.AppImage",
        "tool.desktop",
        "x.jar",
        "a/b/run.EXE",
    ] {
        assert!(is_executable_type(Path::new(name)), "{name}");
        let err = open_with_default_app(Path::new(name)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
    }
    for name in [
        "clip.mp4",
        "photo.jpg",
        "archive.tar.gz",
        "notes.txt",
        "README",
    ] {
        assert!(!is_executable_type(Path::new(name)), "{name}");
    }
}

#[test]
fn save_to_dir_reports_a_missing_source() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(save_to_dir(&tmp.path().join("missing"), tmp.path(), "x").is_err());
}

#[test]
fn links_open_only_as_web_mail_or_phone_addresses() {
    assert_eq!(
        link_url("https://example.org/a?b").as_deref(),
        Some("https://example.org/a?b")
    );
    assert_eq!(
        link_url("example.org/page").as_deref(),
        Some("https://example.org/page")
    );
    assert_eq!(
        link_url("ann@example.org").as_deref(),
        Some("mailto:ann@example.org")
    );
    assert_eq!(
        link_url("+1 (555) 010-99").as_deref(),
        Some("tel:+155501099")
    );
    assert_eq!(
        link_url("tg://resolve?domain=x").as_deref(),
        Some("tg://resolve?domain=x")
    );
    // Local files, scripts and option-like arguments are never handed over
    for refused in [
        "file:///etc/passwd",
        "javascript:alert(1)",
        "ssh://host",
        "-x",
        "two words",
    ] {
        assert_eq!(link_url(refused), None, "{refused}");
    }
}
