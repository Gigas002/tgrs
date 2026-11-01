//! Your active sessions: the devices and apps logged in to your account.

use tdlib_rs::{
    enums::{SessionDeviceType, SessionType},
    types::{Session, UnconfirmedSession},
};

use super::message::format_timestamp;

/// A device or app logged in to your account, as the session list shows it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionEntry {
    pub id: i64,
    /// This app's own session, which logging out ends.
    pub is_current: bool,
    /// Logged in elsewhere but not yet confirmed from a known device.
    pub is_unconfirmed: bool,
    /// Waiting for the two-step verification password.
    pub is_password_pending: bool,
    /// The app and its version, e.g. `Telegram Desktop 5.6`.
    pub application: String,
    /// The device and its system, e.g. `ThinkPad · Linux 6.9`.
    pub device: String,
    /// Where it logged in from, e.g. `Berlin, Germany`; may be empty.
    pub location: String,
    pub ip_address: String,
    /// When it was last used; `online` for this app.
    pub last_active: String,
    /// Whether it's a phone or tablet, a browser or a computer.
    pub device_kind: DeviceKind,
}

/// What kind of device a session is on, for its icon.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DeviceKind {
    Phone,
    Browser,
    #[default]
    Computer,
}

impl DeviceKind {
    pub fn icon(self) -> &'static str {
        match self {
            Self::Phone => "📱",
            Self::Browser => "🌐",
            Self::Computer => "💻",
        }
    }
}

fn device_kind(device_type: &SessionDeviceType) -> DeviceKind {
    match device_type {
        SessionDeviceType::Android | SessionDeviceType::Iphone | SessionDeviceType::Ipad => {
            DeviceKind::Phone
        }
        SessionDeviceType::Brave
        | SessionDeviceType::Chrome
        | SessionDeviceType::Edge
        | SessionDeviceType::Firefox
        | SessionDeviceType::Opera
        | SessionDeviceType::Safari
        | SessionDeviceType::Vivaldi => DeviceKind::Browser,
        _ => DeviceKind::Computer,
    }
}

/// `parts` joined with `sep`, leaving out empty ones.
fn join(parts: &[&str], sep: &str) -> String {
    parts
        .iter()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(sep)
}

impl From<&Session> for SessionEntry {
    fn from(session: &Session) -> Self {
        let system = join(&[&session.platform, &session.system_version], " ");
        SessionEntry {
            id: session.id,
            is_current: session.is_current,
            is_unconfirmed: session.is_unconfirmed,
            is_password_pending: session.is_password_pending,
            application: join(
                &[&session.application_name, &session.application_version],
                " ",
            ),
            device: join(&[&session.device_model, &system], " · "),
            location: session.location.trim().to_owned(),
            ip_address: session.ip_address.trim().to_owned(),
            last_active: if session.is_current {
                "online".to_owned()
            } else {
                format_timestamp(session.last_active_date)
            },
            device_kind: device_kind(&session.device_type),
        }
    }
}

impl SessionEntry {
    /// One line describing the session after its icon: the app, the device,
    /// where and when.
    pub fn summary(&self) -> String {
        let mut summary = join(
            &[
                &self.application,
                &self.device,
                &self.location,
                &self.last_active,
            ],
            " · ",
        );
        if summary.is_empty() {
            summary = "Unknown device".to_owned();
        }
        if self.is_current {
            summary.push_str(" (this app)");
        } else if self.is_unconfirmed {
            summary.push_str(" (new, unconfirmed)");
        } else if self.is_password_pending {
            summary.push_str(" (waiting for password)");
        }
        summary
    }
}

/// The sessions in the order TDLib gives them, which is the order Telegram
/// asks apps to show them in (this app's first).
pub fn session_entries(sessions: &[Session]) -> Vec<SessionEntry> {
    sessions.iter().map(SessionEntry::from).collect()
}

/// A login to your account that hasn't been confirmed yet, to warn about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignInAlert {
    pub device_model: String,
    pub location: String,
}

impl SignInAlert {
    /// The alert for a login from a device; None for a connected business
    /// bot, which is also an unconfirmed session.
    pub fn from_unconfirmed(session: &UnconfirmedSession) -> Option<Self> {
        matches!(session.r#type, SessionType::Device(_)).then(|| SignInAlert {
            device_model: session.device_model.trim().to_owned(),
            location: session.location.trim().to_owned(),
        })
    }

    /// The warning to show, e.g. `New sign-in on Pixel 8 from Berlin, Germany`.
    pub fn text(&self) -> String {
        let device = if self.device_model.is_empty() {
            "a new device"
        } else {
            &self.device_model
        };
        if self.location.is_empty() {
            format!("New sign-in on {device}")
        } else {
            format!("New sign-in on {device} from {}", self.location)
        }
    }
}
