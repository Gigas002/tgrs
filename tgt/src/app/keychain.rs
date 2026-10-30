//! Keeping the TDLib database encrypted with a key from the system keychain,
//! as `[credentials] store` asks.

use std::path::Path;

use libtgt::client::Client;

use crate::{
    app::error::AppError,
    config::CredentialStore,
    credentials::{CredentialsError, Keychain},
};

/// The keychain to use; `None` leaves the database unencrypted.
pub fn open(store: CredentialStore) -> Result<Option<Keychain>, AppError> {
    match store {
        CredentialStore::Plain => Ok(None),
        CredentialStore::Keychain => blocking(Keychain::system)
            .map(Some)
            .map_err(|e| required(&e)),
        CredentialStore::Auto => match blocking(Keychain::system) {
            Ok(keychain) => Ok(Some(keychain)),
            Err(e) => {
                tracing::warn!("keychain unavailable, the TDLib database stays unencrypted: {e}");
                Ok(None)
            }
        },
    }
}

/// The key to open the database with; empty when there is none.
pub fn load_key(
    keychain: &mut Option<Keychain>,
    store: CredentialStore,
    database_dir: &Path,
) -> Result<String, AppError> {
    let Some(chain) = keychain.as_ref() else {
        return Ok(String::new());
    };
    match blocking(|| chain.load_database_key(database_dir)) {
        Ok(key) => Ok(key.unwrap_or_default()),
        Err(e) if store == CredentialStore::Keychain => Err(required(&e)),
        Err(e) => {
            tracing::warn!("failed to read the database key from the keychain: {e}");
            // Don't try to store a new key in a keychain that can't be read
            *keychain = None;
            Ok(String::new())
        }
    }
}

/// Encrypts the unencrypted database of an authorized client with a new key,
/// saved in the keychain first so a crash can't lose it.
pub async fn encrypt_database(
    client: &mut Client,
    keychain: &Keychain,
    store: CredentialStore,
    database_dir: &Path,
) -> Result<(), AppError> {
    let result = encrypt(client, keychain, database_dir).await;
    match result {
        Ok(()) => {
            tracing::info!("encrypted the TDLib database with a key from the keychain");
            Ok(())
        }
        Err(e) if store == CredentialStore::Keychain => Err(e),
        Err(e) => {
            tracing::warn!("the TDLib database stays unencrypted: {e}");
            Ok(())
        }
    }
}

async fn encrypt(
    client: &mut Client,
    keychain: &Keychain,
    database_dir: &Path,
) -> Result<(), AppError> {
    let key = new_key()?;
    blocking(|| keychain.save_database_key(database_dir, &key)).map_err(|e| required(&e))?;
    if let Err(e) = client.set_database_encryption_key(key).await {
        // The database is still unencrypted; on the next start the stale key
        // fails to open it and the database is opened without it.
        if let Err(e) = blocking(|| keychain.delete_database_key(database_dir)) {
            tracing::warn!("failed to remove the unused database key: {e}");
        }
        return Err(AppError::Command(format!(
            "failed to encrypt the TDLib database: {}",
            e.message
        )));
    }
    Ok(())
}

#[cfg(feature = "keychain")]
fn new_key() -> Result<String, AppError> {
    crate::credentials::generate_database_key().map_err(|e| required(&e))
}

#[cfg(not(feature = "keychain"))]
fn new_key() -> Result<String, AppError> {
    Err(required(&CredentialsError::Unsupported))
}

/// Forgets the key of a database that no longer exists (after logging out or
/// removing the data directory). Failures are only logged.
pub fn forget_key(store: CredentialStore, database_dir: &Path) {
    if store == CredentialStore::Plain {
        return;
    }
    let result = blocking(Keychain::system)
        .and_then(|keychain| blocking(|| keychain.delete_database_key(database_dir)));
    if let Err(e) = result {
        tracing::warn!("failed to remove the database key from the keychain: {e}");
    }
}

fn required(e: &CredentialsError) -> AppError {
    AppError::Command(format!(
        "{e} (set `[credentials] store` in config.toml to \"auto\" or \"plain\" to run without the keychain)"
    ))
}

/// Keychain calls block on D-Bus; keep them off the async workers.
fn blocking<T>(f: impl FnOnce() -> T) -> T {
    match tokio::runtime::Handle::try_current() {
        Ok(_) => tokio::task::block_in_place(f),
        Err(_) => f(),
    }
}
