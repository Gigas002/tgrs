use std::{collections::HashMap, ffi::OsString, path::Path};

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::style::{Color, Modifier};

use super::*;
use crate::{
    app::{action::Action, event::Event},
    config::{KeymapFile, ThemeFile, keymap as keymap_file},
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
        self.home().join(".config/tgt")
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

    assert_eq!(settings.dirs.config, home.join(".config/tgt"));
    assert_eq!(settings.dirs.data, home.join(".local/share/tgt"));
    assert_eq!(settings.dirs.state, home.join(".local/state/tgt"));
    assert_eq!(settings.logging.dir, home.join(".local/state/tgt/logs"));
    assert_eq!(
        settings.telegram.database_dir,
        home.join(".local/share/tgt/tdlib")
    );
    assert_eq!(
        settings.tdlib_log.path,
        home.join(".local/state/tgt/tdlib.log")
    );
    assert_eq!(settings.telegram.api_id, DEFAULT_API_ID);
    assert_eq!(settings.telegram.api_hash, DEFAULT_API_HASH);
    assert!(settings.telegram.use_message_database);
    assert_eq!(settings.app.frame_rate, DEFAULT_FRAME_RATE);
    assert_eq!(settings.logging.level, LogLevel::Info);
    assert_eq!(settings.theme.name, DEFAULT_THEME);
    assert_eq!(settings.command, Command::Tui);
}

#[test]
fn xdg_variables_are_respected() {
    let env = Env::new();
    let xdg = env.home().join("xdg");
    let env = env
        .set("XDG_CONFIG_HOME", xdg.join("config"))
        .set("XDG_DATA_HOME", xdg.join("data"))
        .set("XDG_STATE_HOME", xdg.join("state"));
    let settings = env.resolve(CliOverrides::default()).unwrap();
    assert_eq!(settings.dirs.config, xdg.join("config/tgt"));
    assert_eq!(settings.dirs.data, xdg.join("data/tgt"));
    assert_eq!(settings.dirs.state, xdg.join("state/tgt"));
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
        frame_rate = 30.0
        use_emoji_icons = true
        photo_viewer_popup_size = 5.0

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

    assert_eq!(settings.app.frame_rate, 30.0);
    assert!(settings.app.use_emoji_icons);
    assert_eq!(settings.app.photo_viewer_popup_size, 1.0);
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

    env.write_config("config.toml", "[app]\nframe_rate = 0.0\n");
    assert!(matches!(
        env.resolve(CliOverrides::default()),
        Err(SettingsError::FrameRate)
    ));
}

#[test]
fn send_disables_the_message_database() {
    let env = Env::new();
    let cli = CliOverrides {
        command: Command::Send {
            chat: "me".into(),
            message: "hi".into(),
        },
        ..CliOverrides::default()
    };
    assert!(!env.resolve(cli).unwrap().telegram.use_message_database);
}

#[test]
fn theme_precedence_is_cli_config_state_default() {
    let env = Env::new();
    let state_file = env.home().join(".local/state/tgt").join(THEME_STATE_FILE);
    std::fs::create_dir_all(state_file.parent().unwrap()).unwrap();
    std::fs::write(&state_file, "tokyo\n").unwrap();
    assert_eq!(
        env.resolve(CliOverrides::default()).unwrap().theme.name,
        "tokyo"
    );

    env.write_config("config.toml", "[app]\ntheme = \"nord\"\n");
    assert_eq!(
        env.resolve(CliOverrides::default()).unwrap().theme.name,
        "nord"
    );

    let cli = CliOverrides {
        theme: Some("gruvbox".into()),
        ..CliOverrides::default()
    };
    assert_eq!(env.resolve(cli).unwrap().theme.name, "gruvbox");
}

#[test]
fn unknown_explicit_theme_is_an_error_but_stale_state_is_not() {
    let env = Env::new();
    let state_file = env.home().join(".local/state/tgt").join(THEME_STATE_FILE);
    std::fs::create_dir_all(state_file.parent().unwrap()).unwrap();
    std::fs::write(&state_file, "deleted").unwrap();
    assert_eq!(
        env.resolve(CliOverrides::default()).unwrap().theme.name,
        DEFAULT_THEME
    );

    let cli = CliOverrides {
        theme: Some("missing".into()),
        ..CliOverrides::default()
    };
    assert!(matches!(
        env.resolve(cli),
        Err(SettingsError::Theme(ThemeError::Unknown(name))) if name == "missing"
    ));
}

#[test]
fn user_theme_file_is_loaded() {
    let env = Env::new();
    env.write_config(
        "themes/mine.toml",
        "[palette]\naccent = \"#ff0000\"\n[common]\ntimestamp = { fg = \"accent\", bold = true }\n",
    );
    let cli = CliOverrides {
        theme: Some("mine".into()),
        ..CliOverrides::default()
    };
    let theme = env.resolve(cli).unwrap().theme;
    let style = theme.common["timestamp"];
    assert_eq!(style.fg, Some(Color::Rgb(255, 0, 0)));
    assert!(style.add_modifier.contains(Modifier::BOLD));
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
    assert_eq!(theme::parse_color(""), Some(Color::Reset));
    assert_eq!(theme::parse_color("#ff8000"), Some(Color::Rgb(255, 128, 0)));
    assert_eq!(theme::parse_color("#f80"), Some(Color::Rgb(255, 136, 0)));
    assert_eq!(theme::parse_color("1, 2, 3"), Some(Color::Rgb(1, 2, 3)));
    assert_eq!(theme::parse_color("42"), Some(Color::Indexed(42)));
    assert_eq!(theme::parse_color("#12345"), None);
    assert_eq!(theme::parse_color("nope"), None);
}

#[test]
fn theme_resolve_rejects_invalid_colors() {
    let file: ThemeFile = toml::from_str("[common]\ntimestamp = { fg = \"nope\" }\n").unwrap();
    assert_eq!(
        Theme::resolve("x", file),
        Err(ThemeError::InvalidColor {
            theme: "x".into(),
            color: "nope".into()
        })
    );
}

#[test]
fn for_tests_uses_the_default_theme() {
    assert_eq!(Settings::for_tests().theme.name, DEFAULT_THEME);
}
