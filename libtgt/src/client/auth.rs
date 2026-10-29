use std::path::PathBuf;

/// TDLib parameters, resolved by the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TdParams {
    pub api_id: i32,
    pub api_hash: String,
    pub database_dir: PathBuf,
    pub use_file_database: bool,
    pub use_chat_info_database: bool,
    pub use_message_database: bool,
    pub system_language_code: String,
    pub device_model: String,
    pub application_version: String,
    pub proxy: Option<Proxy>,
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
