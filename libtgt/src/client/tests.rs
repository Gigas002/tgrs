use super::*;

#[test]
fn td_params_debug_hides_secrets() {
    let params = TdParams {
        api_id: 42,
        api_hash: "api-hash-secret".into(),
        database_dir: "/data/tg".into(),
        database_encryption_key: "database-key-secret".into(),
        use_file_database: true,
        use_chat_info_database: true,
        use_message_database: true,
        system_language_code: "en".into(),
        device_model: "Desktop".into(),
        application_version: "1.0.0".into(),
        proxy: Some(Proxy {
            server: "proxy.example".into(),
            port: 1080,
            kind: ProxyKind::Socks5 {
                username: "user".into(),
                password: "proxy-password-secret".into(),
            },
        }),
    };

    let debug = format!("{params:?}");
    assert!(!debug.contains("secret"), "{debug}");
    assert!(debug.contains("database_encrypted: true"), "{debug}");
    assert!(debug.contains("proxy.example:1080"), "{debug}");
}

#[test]
fn logging_is_configured_without_a_client() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tdlib.log");
    let logging = TdLogging {
        verbosity_level: 1,
        path: path.clone(),
        redirect_stderr: false,
    };
    configure_logging(&logging).unwrap();

    let level = execute::execute(&serde_json::json!({"@type": "getLogVerbosityLevel"})).unwrap();
    assert_eq!(level["verbosity_level"], 1);
    let stream = execute::execute(&serde_json::json!({"@type": "getLogStream"})).unwrap();
    assert_eq!(stream["path"], path.to_string_lossy().as_ref());

    // Back to TDLib's defaults for other tests
    execute::execute(&serde_json::json!({
        "@type": "setLogStream",
        "log_stream": {"@type": "logStreamDefault"},
    }))
    .unwrap();
}

#[test]
fn execute_reports_errors() {
    let err = execute::execute(&serde_json::json!({"@type": "noSuchRequest"})).unwrap_err();
    assert!(!err.is_empty());
}

#[test]
fn albums_hold_ten_files_split_evenly() {
    let paths = |n: usize| (0..n).map(|i| i.to_string()).collect::<Vec<_>>();
    let sizes = |n| {
        album_groups(&paths(n))
            .iter()
            .map(|g| g.len())
            .collect::<Vec<_>>()
    };
    assert!(sizes(0).is_empty());
    assert_eq!(sizes(1), [1]);
    assert_eq!(sizes(2), [2]);
    assert_eq!(sizes(10), [10]);
    // No album is left with a single file
    assert_eq!(sizes(11), [6, 5]);
    assert_eq!(sizes(21), [7, 7, 7]);
    // The order is kept
    let all = paths(12);
    let groups = album_groups(&all);
    assert_eq!(groups.concat(), all);
}
