use super::*;
use crate::config::{
    keymap,
    theme::{self, ThemeSource},
};

#[test]
fn bundled_config_parses_to_defaults() {
    let (_, content) = BUNDLED_FILES
        .iter()
        .find(|(path, _)| *path == FILE_NAME)
        .unwrap();
    // Every key in the bundled file is commented out.
    assert_eq!(parse(content).unwrap(), FileConfig::default());
}

#[test]
fn bundled_config_parses_when_uncommented() {
    let (_, content) = BUNDLED_FILES
        .iter()
        .find(|(path, _)| *path == FILE_NAME)
        .unwrap();
    let uncommented: String = content
        .lines()
        .map(|line| match line.strip_prefix("# ") {
            Some(rest) if rest.contains(" = ") || rest.starts_with('[') => rest,
            _ => line,
        })
        .collect::<Vec<_>>()
        .join("\n");

    let file = parse(&uncommented).unwrap();
    assert_eq!(file.app.frame_rate, Some(60.0));
    assert_eq!(file.app.theme.as_deref(), Some("theme"));
    assert_eq!(file.logger.level, Some(LogLevel::Info));
    assert_eq!(file.logger.rotation, Some(Rotation::Daily));
    assert_eq!(file.telegram.api_id, Some(94575));
    let proxy = file.telegram.proxy.unwrap();
    assert_eq!(proxy.r#type, Some(ProxyType::Socks5));
    assert_eq!(proxy.port, Some(1080));
}

#[test]
fn parse_ignores_unknown_keys() {
    let file = parse("[app]\nunknown = 1\nmouse_support = false\n").unwrap();
    assert_eq!(file.app.mouse_support, Some(false));
}

#[test]
fn parse_rejects_invalid_enum_values() {
    assert!(parse("[logger]\nlevel = \"loud\"\n").is_err());
    assert!(parse("[telegram.proxy]\ntype = \"ftp\"\n").is_err());
}

#[test]
fn load_missing_file_is_none() {
    let dir = tempfile::tempdir().unwrap();
    assert!(load(dir.path()).unwrap().is_none());
}

#[test]
fn load_reports_parse_errors_with_path() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(FILE_NAME), "[app\n").unwrap();
    let err = load(dir.path()).unwrap_err();
    assert!(matches!(err, ConfigError::Parse { .. }));
    assert!(err.to_string().contains(FILE_NAME));
}

#[test]
fn bundled_keymap_has_core_window_bindings() {
    let file = keymap::bundled();
    assert!(!file.core_window.unwrap().keymap.is_empty());
    assert!(!file.prompt.unwrap().keymap.is_empty());
}

#[test]
fn bundled_themes_parse() {
    for (name, source) in theme::discover(std::path::Path::new("/nonexistent")) {
        let file = source.load().unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!file.palette.is_empty(), "{name} has a palette");
    }
}

#[test]
fn discover_puts_default_first_and_user_files_shadow_bundled() {
    let dir = tempfile::tempdir().unwrap();
    let themes_dir = dir.path().join(theme::DIR_NAME);
    std::fs::create_dir(&themes_dir).unwrap();
    std::fs::write(themes_dir.join("nord.toml"), "").unwrap();
    std::fs::write(themes_dir.join("mine.toml"), "").unwrap();
    std::fs::write(themes_dir.join("notes.txt"), "").unwrap();

    let themes = theme::discover(dir.path());
    let names: Vec<&str> = themes.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names.first(), Some(&"theme"));
    assert!(names.contains(&"mine"));
    assert!(!names.contains(&"notes"));
    assert_eq!(names.iter().filter(|name| **name == "nord").count(), 1);

    let nord = themes.iter().find(|(name, _)| name == "nord").unwrap();
    assert_eq!(nord.1, ThemeSource::File(themes_dir.join("nord.toml")));

    let mut sorted = names[1..].to_vec();
    sorted.sort_unstable();
    assert_eq!(names[1..], sorted[..]);
}
