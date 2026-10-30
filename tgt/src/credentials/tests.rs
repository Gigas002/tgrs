use std::path::Path;

use super::*;

#[test]
fn account_is_per_database_dir() {
    assert_ne!(
        account(Path::new("/a/tdlib")),
        account(Path::new("/b/tdlib"))
    );
    assert_eq!(
        account(Path::new("/a/tdlib")),
        "tdlib-database-key:/a/tdlib"
    );
}

#[cfg(feature = "keychain")]
mod keychain {
    use super::*;

    #[test]
    fn generated_keys_are_base64_and_random() {
        let key = generate_database_key().unwrap();
        assert_eq!(key.len(), 64);
        assert!(
            key.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/')
        );
        assert_ne!(key, generate_database_key().unwrap());
    }

    #[test]
    fn database_key_round_trip() {
        let keychain = Keychain::mock();
        let dir = Path::new("/data/tdlib");
        assert_eq!(keychain.load_database_key(dir).unwrap(), None);

        keychain.save_database_key(dir, "secret").unwrap();
        assert_eq!(
            keychain.load_database_key(dir).unwrap().as_deref(),
            Some("secret")
        );
        assert_eq!(
            keychain.load_database_key(Path::new("/other")).unwrap(),
            None
        );

        keychain.delete_database_key(dir).unwrap();
        assert_eq!(keychain.load_database_key(dir).unwrap(), None);
        // Deleting twice is fine
        keychain.delete_database_key(dir).unwrap();
    }
}

#[cfg(not(feature = "keychain"))]
#[test]
fn keychain_is_unsupported_without_the_feature() {
    assert!(matches!(
        Keychain::system(),
        Err(CredentialsError::Unsupported)
    ));
}
