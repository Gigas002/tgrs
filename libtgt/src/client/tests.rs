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
