use super::*;
use crate::config::{
    keymap,
    theme::{self, BorderShape, BorderSurface, HighlightGroup, HighlightLink},
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
    assert_eq!(file.logger.level, Some(LogLevel::Info));
    assert_eq!(file.logger.rotation, Some(Rotation::Daily));
    assert_eq!(file.telegram.api_id, Some(94575));
    let proxy = file.telegram.proxy.unwrap();
    assert_eq!(proxy.r#type, Some(ProxyType::Socks5));
    assert_eq!(proxy.port, Some(1080));
    assert_eq!(file.notifications.enabled, Some(true));
    assert_eq!(file.notifications.show_preview, Some(true));
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
fn theme_file_overrides_groups_borders_and_marker() {
    let table: toml::Table = toml::from_str(
        r##"
[highlight.Normal]
foreground = "white"
bold = true

[highlight.SelectedRow]
link = "none"
background = "#202020"

[ui.border]
default = "double"
message = "plain"

[ui.indicator]
selection = "> "
"##,
    )
    .unwrap();
    let (file, warnings) = theme::parse(&table);
    assert!(warnings.is_empty(), "{warnings:?}");
    let normal = &file.highlights()[&HighlightGroup::Normal];
    assert_eq!(normal.foreground.as_deref(), Some("white"));
    assert_eq!(normal.bold, Some(true));
    let selected = &file.highlights()[&HighlightGroup::SelectedRow];
    assert_eq!(selected.link, Some(HighlightLink::Detached));
    assert_eq!(file.border_shapes().default, Some(BorderShape::Double));
    assert_eq!(
        file.border_shapes().get(BorderSurface::Message),
        Some(BorderShape::Plain)
    );
    assert_eq!(file.selection_marker(), Some("> "));
}

#[test]
fn bad_theme_entries_are_skipped_with_warnings() {
    let table: toml::Table = toml::from_str(
        r#"
[highlight.NoSuchGroup]
bold = true

[highlight.Normal]
bold = "yes"
link = "Nope"
sparkle = true
italic = true

[ui.border]
pane = "wavy"

[ui.indicator]
selection = ""

[palette]
"#,
    )
    .unwrap();
    let (file, warnings) = theme::parse(&table);
    assert_eq!(warnings.len(), 7, "{warnings:#?}");
    // Valid siblings still apply
    assert_eq!(
        file.highlights()[&HighlightGroup::Normal].italic,
        Some(true)
    );
    assert_eq!(file.selection_marker(), None);
}

#[test]
fn missing_theme_file_is_the_default_theme() {
    let dir = tempfile::tempdir().unwrap();
    let (file, warnings) = theme::load(dir.path()).unwrap();
    assert_eq!(file, theme::ThemeFile::default());
    assert!(warnings.is_empty());
}

#[test]
fn example_theme_documents_every_group() {
    let example = include_str!("../../../examples/theme.toml");
    for group in HighlightGroup::ALL {
        let header = format!("# [highlight.{}] ", group.name());
        assert!(example.contains(&header), "{} is documented", group.name());
    }
    for surface in ["pane", "composer", "modal", "picker", "message"] {
        assert!(example.contains(&format!("# {surface} = ")), "{surface}");
        assert!(BorderSurface::from_name(surface).is_some());
    }
}
