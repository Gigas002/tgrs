use std::{fmt, path::PathBuf};

/// TDLib parameters, resolved by the caller.
#[derive(Clone, PartialEq, Eq)]
pub struct TdParams {
    pub api_id: i32,
    pub api_hash: String,
    pub database_dir: PathBuf,
    /// Key the TDLib database is encrypted with; empty means unencrypted.
    pub database_encryption_key: String,
    pub use_file_database: bool,
    pub use_chat_info_database: bool,
    pub use_message_database: bool,
    /// Telegram's test servers: separate accounts, for development.
    pub use_test_dc: bool,
    pub system_language_code: String,
    pub device_model: String,
    pub application_version: String,
    pub proxy: Option<Proxy>,
}

/// Leaves out the secrets: the database key, the API hash and proxy credentials.
impl fmt::Debug for TdParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TdParams")
            .field("api_id", &self.api_id)
            .field("api_hash", &"<redacted>")
            .field("database_dir", &self.database_dir)
            .field(
                "database_encrypted",
                &!self.database_encryption_key.is_empty(),
            )
            .field("use_file_database", &self.use_file_database)
            .field("use_chat_info_database", &self.use_chat_info_database)
            .field("use_message_database", &self.use_message_database)
            .field("use_test_dc", &self.use_test_dc)
            .field("system_language_code", &self.system_language_code)
            .field("device_model", &self.device_model)
            .field("application_version", &self.application_version)
            .field(
                "proxy",
                &self
                    .proxy
                    .as_ref()
                    .map(|p| format!("{}:{}", p.server, p.port)),
            )
            .finish()
    }
}

/// TDLib's own log, separate from the application log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TdLogging {
    pub verbosity_level: i32,
    pub path: PathBuf,
    pub redirect_stderr: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proxy {
    pub server: String,
    pub port: i32,
    pub kind: ProxyKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProxyKind {
    Socks5 {
        username: String,
        password: String,
    },
    Http {
        username: String,
        password: String,
        /// The proxy supports only HTTP requests.
        http_only: bool,
    },
    Mtproto {
        /// Hex-encoded secret.
        secret: String,
    },
}

impl From<&Proxy> for tdlib_rs::types::Proxy {
    fn from(proxy: &Proxy) -> Self {
        let r#type = match &proxy.kind {
            ProxyKind::Socks5 { username, password } => {
                tdlib_rs::enums::ProxyType::Socks5(tdlib_rs::types::ProxyTypeSocks5 {
                    username: username.clone(),
                    password: password.clone(),
                })
            }
            ProxyKind::Http {
                username,
                password,
                http_only,
            } => tdlib_rs::enums::ProxyType::Http(tdlib_rs::types::ProxyTypeHttp {
                username: username.clone(),
                password: password.clone(),
                http_only: *http_only,
            }),
            ProxyKind::Mtproto { secret } => {
                tdlib_rs::enums::ProxyType::Mtproto(tdlib_rs::types::ProxyTypeMtproto {
                    secret: secret.clone(),
                })
            }
        };
        Self {
            server: proxy.server.clone(),
            port: proxy.port,
            r#type,
        }
    }
}

/// Answers TDLib's interactive login steps (phone number, codes, password, ...).
///
/// The library has no terminal of its own; the binary decides how to ask.
pub trait Authenticator {
    fn phone_number(&mut self) -> String;
    fn code(&mut self) -> String;
    fn email_address(&mut self) -> String;
    fn email_code(&mut self) -> String;
    fn password(&mut self) -> String;
    /// First and last name for a new account.
    fn registration(&mut self) -> (String, String);
    /// The login has to be confirmed with this link on another device.
    fn confirm_link(&mut self, link: &str);
    /// TDLib rejected the last answer.
    fn report_error(&mut self, message: &str);
}
