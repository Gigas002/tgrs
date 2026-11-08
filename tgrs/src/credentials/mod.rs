//! Secrets kept outside config files: the key of the encrypted TDLib database,
//! stored in the system keychain (Secret Service).
//!
//! TDLib keeps the login session in its database; encrypting it with a key
//! that only the keychain knows keeps the session safe at rest.

use std::path::Path;

#[cfg(test)]
mod tests;

/// Keychain service name of tgt's entries.
pub const SERVICE: &str = "tgt";

#[derive(Debug, thiserror::Error)]
pub enum CredentialsError {
    #[cfg(feature = "keychain")]
    #[error("keychain: {0}")]
    Keychain(#[from] keyring_core::Error),

    #[cfg(not(feature = "keychain"))]
    #[error("tgt was built without keychain support (the `keychain` feature)")]
    Unsupported,

    #[cfg(feature = "keychain")]
    #[error("failed to generate a database key: {0}")]
    Random(getrandom::Error),
}

/// A credential store holding one database key per TDLib database directory.
pub struct Keychain {
    #[cfg(feature = "keychain")]
    store: std::sync::Arc<keyring_core::CredentialStore>,
}

impl std::fmt::Debug for Keychain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Keychain").finish_non_exhaustive()
    }
}

#[cfg(feature = "keychain")]
impl Keychain {
    /// The system keychain, through the Secret Service D-Bus API.
    pub fn system() -> Result<Self, CredentialsError> {
        Ok(Self {
            store: dbus_secret_service_keyring_store::Store::new()?,
        })
    }

    /// An in-memory store for tests.
    #[cfg(test)]
    pub(crate) fn mock() -> Self {
        Self {
            store: keyring_core::mock::Store::new().expect("mock store"),
        }
    }

    /// The key of the database in `database_dir`, if one was saved.
    pub fn load_database_key(
        &self,
        database_dir: &Path,
    ) -> Result<Option<String>, CredentialsError> {
        match self.entry(database_dir)?.get_password() {
            Ok(key) => Ok(Some(key)),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save_database_key(
        &self,
        database_dir: &Path,
        key: &str,
    ) -> Result<(), CredentialsError> {
        Ok(self.entry(database_dir)?.set_password(key)?)
    }

    /// Forgets the key; a missing entry is not an error.
    pub fn delete_database_key(&self, database_dir: &Path) -> Result<(), CredentialsError> {
        match self.entry(database_dir)?.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn entry(&self, database_dir: &Path) -> Result<keyring_core::Entry, CredentialsError> {
        Ok(self.store.build(SERVICE, &account(database_dir), None)?)
    }
}

#[cfg(not(feature = "keychain"))]
impl Keychain {
    pub fn system() -> Result<Self, CredentialsError> {
        Err(CredentialsError::Unsupported)
    }

    pub fn load_database_key(&self, _: &Path) -> Result<Option<String>, CredentialsError> {
        Err(CredentialsError::Unsupported)
    }

    pub fn save_database_key(&self, _: &Path, _: &str) -> Result<(), CredentialsError> {
        Err(CredentialsError::Unsupported)
    }

    pub fn delete_database_key(&self, _: &Path) -> Result<(), CredentialsError> {
        Err(CredentialsError::Unsupported)
    }
}

/// Keychain account of a database: its directory, so several databases
/// (`telegram.database_dir`) don't share a key.
pub fn account(database_dir: &Path) -> String {
    format!("tdlib-database-key:{}", database_dir.display())
}

/// A random database key. TDLib takes the key as base64 bytes; any string of
/// base64 characters with a length divisible by four is valid base64.
#[cfg(feature = "keychain")]
pub fn generate_database_key() -> Result<String, CredentialsError> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    // 64 characters encode 48 random bytes
    let mut bytes = [0u8; 64];
    getrandom::fill(&mut bytes).map_err(CredentialsError::Random)?;
    Ok(bytes
        .iter()
        .map(|b| char::from(ALPHABET[usize::from(b & 63)]))
        .collect())
}
