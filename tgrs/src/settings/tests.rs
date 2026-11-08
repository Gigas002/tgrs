use std::{collections::HashMap, ffi::OsString, path::Path};

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::style::{Color, Modifier};

use super::*;
use crate::{
    app::{action::Action, event::Event},
    config::{KeymapFile, keymap as keymap_file},
    ui::component_name::ComponentName,
};

/// An isolated home with `vars` as the environment.
struct Env {
    home: tempfile::TempDir,
    vars: HashMap<String, OsString>,
}

impl Env {
    fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let vars = HashMap::from([("HOME".to_owned(), home.path().into())]);
        Self { home, vars }
    }

    fn set(mut self, key: &str, value: impl Into<OsString>) -> Self {
        self.vars.insert(key.to_owned(), value.into());
        self
    }

    fn home(&self) -> &Path {
        self.home.path()
    }

    fn config_dir(&self) -> PathBuf {
        self.home().join(".config/tgrs")
    }

    fn write_config(&self, name: &str, content: &str) {
        let path = self.config_dir().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    fn resolve(&self, cli: CliOverrides) -> Result<Settings, SettingsError> {
        Settings::resolve(cli, |key| self.vars.get(key).cloned())
    }
}

#[test]
fn defaults_follow_xdg() {
    let env = Env::new();
    let settings = env.resolve(CliOverrides::default()).unwrap();
    let home = env.home();

    assert_eq!(settings.dirs.config, home.join(".config/tgrs"));
    assert_eq!(settings.dirs.data, home.join(".local/share/tgrs"));
    assert_eq!(settings.dirs.state, home.join(".local/state/tgrs"));
    assert_eq!(settings.logging.dir, home.join(".local/state/tgrs/logs"));
    assert_eq!(
        settings.telegram.database_dir,
        home.join(".local/share/tgrs/tdlib")
    );
    assert_eq!(
        settings.tdlib_log.path,
        home.join(".local/state/tgrs/tdlib.log")
    );
    assert_eq!(settings.telegram.api_id, DEFAULT_API_ID);
    assert_eq!(settings.telegram.api_hash, DEFAULT_API_HASH);
    assert!(settings.telegram.use_message_database);
    assert_eq!(settings.logging.level, LogLevel::Info);
    assert_eq!(settings.theme, Theme::default());
    assert!(settings.warnings.is_empty());
    assert_eq!(settings.app.download_dir, home.join("Downloads"));
    assert_eq!(settings.credentials, CredentialStore::Auto);
    assert!(settings.telegram.database_encryption_key.is_empty());
    assert_eq!(settings.command, Command::Tui);
}

#[test]
fn xdg_variables_are_respected() {
    let env = Env::new();
    let xdg = env.home().join("xdg");
    let env = env
        .set("XDG_CONFIG_HOME", xdg.join("config"))
        .set("XDG_DATA_HOME", xdg.join("data"))
        .set("XDG_STATE_HOME", xdg.join("state"))
        .set("XDG_DOWNLOAD_DIR", xdg.join("downloads"));
    let settings = env.resolve(CliOverrides::default()).unwrap();
    assert_eq!(settings.dirs.config, xdg.join("config/tgrs"));
    assert_eq!(settings.dirs.data, xdg.join("data/tgrs"));
    assert_eq!(settings.dirs.state, xdg.join("state/tgrs"));
    assert_eq!(settings.app.download_dir, xdg.join("downloads"));
}

#[test]
fn missing_home_is_an_error() {
    let err = Settings::resolve(CliOverrides::default(), |_| None).unwrap_err();
    assert!(matches!(err, SettingsError::Paths(PathsError::NoHome)));
}

#[test]
fn config_file_values_apply() {
    let env = Env::new();
    env.write_config(
        "config.toml",
        r#"
        [app]
        use_emoji_icons = true
        download_dir = "~/dl"

        [credentials]
        store = "plain"

        [logger]
        level = "debug"
        dir = "~/logs"
        rotation = "never"
        keep = 3

        [telegram]
        api_id = 1
        api_hash = "hash"
        database_dir = "~/db"
        log_verbosity = 4

        [telegram.proxy]
        type = "mtproto"
        server = "proxy.example"
        secret = "abcd"
        "#,
    );
    let settings = env.resolve(CliOverrides::default()).unwrap();
    let home = env.home();

    assert!(settings.app.use_emoji_icons);
    assert_eq!(settings.app.download_dir, home.join("dl"));
    assert_eq!(settings.credentials, CredentialStore::Plain);
    assert_eq!(settings.logging.level, LogLevel::Debug);
    assert_eq!(settings.logging.dir, home.join("logs"));
    assert_eq!(settings.logging.rotation, Rotation::Never);
    assert_eq!(settings.logging.keep, 3);
    assert_eq!(settings.telegram.api_id, 1);
    assert_eq!(settings.telegram.api_hash, "hash");
    assert_eq!(settings.telegram.database_dir, home.join("db"));
    assert_eq!(settings.tdlib_log.verbosity_level, 4);
    assert_eq!(
        settings.telegram.proxy,
        Some(Proxy {
            server: "proxy.example".into(),
            port: DEFAULT_PROXY_PORT,
            kind: ProxyKind::Mtproto {
                secret: "abcd".into()
            },
        })
    );
}

#[test]
fn proxy_without_server_is_ignored() {
    let env = Env::new();
    env.write_config("config.toml", "[telegram.proxy]\ntype = \"http\"\n");
    assert_eq!(
        env.resolve(CliOverrides::default()).unwrap().telegram.proxy,
        None
    );
}

#[test]
fn cli_overrides_config() {
    let env = Env::new();
    env.write_config(
        "config.toml",
        "[logger]\nlevel = \"debug\"\ndir = \"~/a\"\n",
    );
    let cli = CliOverrides {
        log_level: Some(LogLevel::Error),
        log_dir: Some(env.home().join("b")),
        ..CliOverrides::default()
    };
    let settings = env.resolve(cli).unwrap();
    assert_eq!(settings.logging.level, LogLevel::Error);
    assert_eq!(settings.logging.dir, env.home().join("b"));
}

#[test]
fn config_dir_from_cli_then_env() {
    let env = Env::new();
    let from_env = env.home().join("env-config");
    let env = env.set(CONFIG_DIR_ENV, &from_env);
    assert_eq!(
        env.resolve(CliOverrides::default()).unwrap().dirs.config,
        from_env
    );

    let from_cli = env.home().join("cli-config");
    let cli = CliOverrides {
        config_dir: Some(from_cli.clone()),
        ..CliOverrides::default()
    };
    assert_eq!(env.resolve(cli).unwrap().dirs.config, from_cli);
}

#[test]
fn api_credentials_from_env_override_config() {
    let env = Env::new()
        .set(API_ID_ENV, " 42 ")
        .set(API_HASH_ENV, "env-hash");
    env.write_config(
        "config.toml",
        "[telegram]\napi_id = 1\napi_hash = \"file\"\n",
    );
    let settings = env.resolve(CliOverrides::default()).unwrap();
    assert_eq!(settings.telegram.api_id, 42);
    assert_eq!(settings.telegram.api_hash, "env-hash");
}

#[test]
fn invalid_api_id_is_an_error() {
    let env = Env::new().set(API_ID_ENV, "nope");
    assert!(matches!(
        env.resolve(CliOverrides::default()),
        Err(SettingsError::ApiId(value)) if value == "nope"
    ));
}

#[test]
fn relative_config_paths_are_rejected() {
    let env = Env::new();
    env.write_config("config.toml", "[telegram]\ndatabase_dir = \"db\"\n");
    assert!(matches!(
        env.resolve(CliOverrides::default()),
        Err(SettingsError::RelativePath {
            key: "telegram.database_dir",
            ..
        })
    ));
}

#[test]
fn invalid_numbers_are_rejected() {
    let env = Env::new();
    env.write_config("config.toml", "[logger]\nkeep = 0\n");
    assert!(matches!(
        env.resolve(CliOverrides::default()),
        Err(SettingsError::LogKeep)
    ));
}

#[test]
fn relative_download_dir_is_rejected() {
    let env = Env::new();
    env.write_config("config.toml", "[app]\ndownload_dir = \"dl\"\n");
    assert!(matches!(
        env.resolve(CliOverrides::default()),
        Err(SettingsError::RelativePath {
            key: "app.download_dir",
            ..
        })
    ));
}

#[test]
fn invalid_keymap_is_an_error() {
    let env = Env::new();
    env.write_config(
        "keymap.toml",
        "[chat]\nkeymap = [{ keys = [\"q\"], command = \"no_such_command\" }]\n",
    );
    assert!(matches!(
        env.resolve(CliOverrides::default()),
        Err(SettingsError::Keymap(KeymapError::UnknownCommand {
            section: "chat",
            ..
        }))
    ));
}

fn key(code: char, modifiers: KeyModifiers) -> Event {
    Event::Key(KeyCode::Char(code), modifiers)
}

fn user_keymap(toml: &str) -> KeymapFile {
    toml::from_str(toml).unwrap()
}

fn single(binding: Option<&ActionBinding>) -> Option<&Action> {
    match binding? {
        ActionBinding::Single { action, .. } => Some(action),
        ActionBinding::Multiple(_) => None,
    }
}

#[test]
fn bundled_keymap_resolves() {
    let keymap = Keymap::resolve(&keymap_file::bundled(), None).unwrap();
    assert_eq!(
        single(keymap.core_window().get(&key('q', KeyModifiers::NONE))),
        Some(&Action::TryQuit)
    );
    // Components inherit core_window bindings.
    let chat = keymap.get_map_of(Some(ComponentName::Chat));
    assert_eq!(
        single(chat.get(&key('q', KeyModifiers::NONE))),
        Some(&Action::TryQuit)
    );
}

#[test]
fn prompt_does_not_inherit_try_quit() {
    let keymap = Keymap::resolve(&keymap_file::bundled(), None).unwrap();
    let prompt = keymap.get_map_of(Some(ComponentName::Prompt));
    assert!(!prompt.contains_key(&key('q', KeyModifiers::NONE)));
}

#[test]
fn user_binding_replaces_default_for_the_same_command() {
    let user = user_keymap(
        r#"
        [prompt]
        keymap = [{ keys = ["ctrl+enter"], command = "prompt_send_message" }]
        "#,
    );
    let keymap = Keymap::resolve(&keymap_file::bundled(), Some(&user)).unwrap();
    let prompt = keymap.get_map_of(Some(ComponentName::Prompt));

    assert!(!prompt.contains_key(&Event::Key(KeyCode::Enter, KeyModifiers::ALT)));
    assert_eq!(
        single(prompt.get(&Event::Key(KeyCode::Enter, KeyModifiers::CONTROL))),
        Some(&Action::PromptSendMessage)
    );
}

#[test]
fn key_sequences_build_nested_maps() {
    let user = user_keymap(
        r#"
        [core_window]
        keymap = [{ keys = ["w", "w"], command = "quit" }]
        "#,
    );
    let keymap = Keymap::resolve(&KeymapFile::default(), Some(&user)).unwrap();
    match keymap.core_window().get(&key('w', KeyModifiers::NONE)) {
        Some(ActionBinding::Multiple(next)) => {
            assert_eq!(
                single(next.get(&key('w', KeyModifiers::NONE))),
                Some(&Action::Quit)
            );
        }
        other => panic!("expected a sequence, got {other:?}"),
    }
}

#[test]
fn unknown_key_is_an_error() {
    let user = user_keymap(
        r#"
        [chat]
        keymap = [{ keys = ["not_a_key"], command = "quit" }]
        "#,
    );
    assert!(matches!(
        Keymap::resolve(&KeymapFile::default(), Some(&user)),
        Err(KeymapError::UnknownKey {
            section: "chat",
            ..
        })
    ));
}

#[test]
fn get_key_of_single_action_finds_bindings() {
    let keymap = Keymap::resolve(&keymap_file::bundled(), None).unwrap();
    let keys = keymap.get_key_of_single_action(ComponentName::Chat, Action::TryQuit);
    assert!(keys.contains(&key('q', KeyModifiers::NONE)));
}

#[test]
fn parse_color_formats() {
    assert_eq!(theme::parse_color("red"), Some(Color::Red));
    assert_eq!(theme::parse_color("dark_gray"), Some(Color::DarkGray));
    assert_eq!(theme::parse_color("terminal_default"), Some(Color::Reset));
    assert_eq!(theme::parse_color("#ff8000"), Some(Color::Rgb(255, 128, 0)));
    assert_eq!(theme::parse_color("ff8000"), Some(Color::Rgb(255, 128, 0)));
    assert_eq!(theme::parse_color("#f80"), None);
    assert_eq!(theme::parse_color("reset"), None);
    assert_eq!(theme::parse_color(""), None);
}

#[test]
fn default_theme_follows_the_terminal() {
    let theme = Theme::default();
    let normal = theme.style(HighlightGroup::Normal);
    assert_eq!(
        (normal.fg, normal.bg),
        (Some(Color::Reset), Some(Color::Reset))
    );
    // Muted text is dimmed terminal text, not a fixed color
    let timestamp = theme.style(HighlightGroup::Timestamp);
    assert_eq!(timestamp.fg, None);
    assert!(timestamp.add_modifier.contains(Modifier::DIM));
    // Selection keeps the cell background
    assert!(theme.background_is_cleared(HighlightGroup::SelectedRow));
    assert_eq!(
        theme.border_type(BorderSurface::Pane),
        ratatui::widgets::BorderType::Plain
    );
    assert_eq!(
        theme.border_type(BorderSurface::Message),
        ratatui::widgets::BorderType::Rounded
    );
    assert_eq!(theme.selection_marker(), theme::DEFAULT_SELECTION_MARKER);
}

#[test]
fn theme_overrides_inherit_through_links() {
    let env = Env::new();
    env.write_config(
        "theme.toml",
        r##"
[highlight.Muted]
foreground = "#808080"

[highlight.Timestamp]
italic = true

[highlight.FocusBorder]
foreground = "light_magenta"

[highlight.Error]
foreground = "not-a-color"

[ui.border]
default = "rounded"
pane = "thick"
"##,
    );
    let settings = env.resolve(CliOverrides::default()).unwrap();
    let theme = &settings.theme;
    let timestamp = theme.style(HighlightGroup::Timestamp);
    assert_eq!(
        timestamp.fg,
        Some(Color::Rgb(128, 128, 128)),
        "linked to Muted"
    );
    assert!(
        timestamp
            .add_modifier
            .contains(Modifier::DIM | Modifier::ITALIC)
    );
    // A child adds to its parent: bold from FocusedPaneBorder, color from FocusBorder
    let focused = theme.style(HighlightGroup::FocusedPaneBorder);
    assert_eq!(focused.fg, Some(Color::LightMagenta));
    assert!(focused.add_modifier.contains(Modifier::BOLD));
    assert_eq!(theme.style(HighlightGroup::Error).fg, Some(Color::Red));
    assert_eq!(settings.warnings.len(), 1, "{:?}", settings.warnings);
    assert_eq!(
        theme.border_type(BorderSurface::Pane),
        ratatui::widgets::BorderType::Thick
    );
    assert_eq!(
        theme.border_type(BorderSurface::Modal),
        ratatui::widgets::BorderType::Rounded
    );
}

#[test]
fn link_cycles_are_reported_not_fatal() {
    let table: toml::Table = toml::from_str(
        "[highlight.Strong]\nlink = \"Title\"\n[highlight.Title]\nlink = \"Strong\"\n",
    )
    .unwrap();
    let (file, parse_warnings) = crate::config::theme::parse(&table);
    assert!(parse_warnings.is_empty());
    let mut warnings = Vec::new();
    let theme = Theme::resolve(&file, &mut warnings);
    assert!(warnings.iter().any(|w| w.contains("cycle")), "{warnings:?}");
    assert_eq!(theme.style(HighlightGroup::Normal).fg, Some(Color::Reset));
}

#[test]
fn media_defaults_load_previews_and_keep_the_cache() {
    let media = Env::new().resolve(CliOverrides::default()).unwrap().media;
    assert!(media.auto_download_previews);
    assert!(media.auto_downloads(i64::MAX));
    assert!(!media.trims_cache());
}

#[test]
fn media_values_apply() {
    let env = Env::new();
    env.write_config(
        "config.toml",
        r#"
        [media]
        auto_download_previews = true
        max_preview_size_mb = 2
        cache_max_size_mb = 500
        cache_max_age_days = 7
        "#,
    );
    let media = env.resolve(CliOverrides::default()).unwrap().media;
    assert_eq!(media.max_preview_size, 2 * 1024 * 1024);
    assert!(media.auto_downloads(2 * 1024 * 1024));
    assert!(!media.auto_downloads(2 * 1024 * 1024 + 1));
    assert_eq!(media.cache_max_size, 500 * 1024 * 1024);
    assert_eq!(media.cache_max_age, 7 * 86_400);
    assert!(media.trims_cache());
}

#[test]
fn previews_can_be_left_to_load_by_hand() {
    let env = Env::new();
    env.write_config("config.toml", "[media]\nauto_download_previews = false\n");
    let media = env.resolve(CliOverrides::default()).unwrap().media;
    assert!(!media.auto_downloads(1));
}

#[test]
fn notification_defaults() {
    let notifications = Env::new()
        .resolve(CliOverrides::default())
        .unwrap()
        .notifications;
    assert!(notifications.enabled);
    assert!(notifications.show_preview);
    assert_eq!(notifications.sound, None);
    assert_eq!(notifications.quiet_hours, None);
    assert!(notifications.open_on_click);
    assert!(!notifications.unread_in_title);
}

#[test]
fn notification_values_apply() {
    let env = Env::new();
    env.write_config(
        "config.toml",
        r#"
        [notifications]
        sound = "message-new-instant"
        quiet_hours = "22:00-07:30"
        open_on_click = false
        unread_in_title = true
        "#,
    );
    let notifications = env.resolve(CliOverrides::default()).unwrap().notifications;
    assert_eq!(notifications.sound.as_deref(), Some("message-new-instant"));
    assert_eq!(
        notifications.quiet_hours,
        Some(QuietHours {
            start: 22 * 60,
            end: 7 * 60 + 30
        })
    );
    assert!(!notifications.open_on_click);
    assert!(notifications.unread_in_title);
}

#[test]
fn malformed_quiet_hours_are_an_error() {
    for bad in ["22-07", "25:00-07:00", "22:00-22:00", "22:00", "7:00-8:00"] {
        let env = Env::new();
        env.write_config(
            "config.toml",
            &format!("[notifications]\nquiet_hours = \"{bad}\"\n"),
        );
        let err = env.resolve(CliOverrides::default()).unwrap_err();
        assert!(matches!(err, SettingsError::QuietHours(_)), "{bad}: {err}");
    }
}

#[test]
fn quiet_hours_within_a_day() {
    let hours = QuietHours::parse("12:00-13:00").unwrap();
    assert!(hours.contains(12 * 60));
    assert!(!hours.contains(13 * 60));
    assert!(!hours.contains(0));
}

#[test]
fn privacy_defaults_and_values() {
    let defaults = Env::new().resolve(CliOverrides::default()).unwrap().privacy;
    assert!(defaults.appear_online);
    assert!(defaults.send_typing);
    let env = Env::new();
    env.write_config(
        "config.toml",
        "[privacy]\nappear_online = false\nsend_typing = false\n",
    );
    let privacy = env.resolve(CliOverrides::default()).unwrap().privacy;
    assert!(!privacy.appear_online);
    assert!(!privacy.send_typing);
}
