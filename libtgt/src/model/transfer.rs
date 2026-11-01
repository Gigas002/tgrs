//! Downloads of message media that the user started, to follow and control.

use super::message::{Media, format_size};

/// What to do with a file once it's downloaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferPurpose {
    /// Open it with the default application.
    Open,
    /// Copy it to the download directory.
    Save,
}

/// Where a download stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferState {
    Downloading,
    /// Stopped by the user; what was downloaded is kept to resume.
    Paused,
    Done,
    /// Stopped by the user and its partial data deleted.
    Cancelled,
    /// TDLib couldn't download it; why.
    Failed(String),
}

/// A download, as the downloads popup shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transfer {
    pub file_id: i32,
    /// The media, to start it again on resume.
    pub media: Media,
    pub purpose: TransferPurpose,
    pub name: String,
    /// Bytes downloaded so far.
    pub downloaded: i64,
    /// Total bytes; 0 when unknown.
    pub size: i64,
    pub state: TransferState,
    /// Order of starting, newer first in the list.
    pub started: u64,
}

impl Transfer {
    pub fn new(media: Media, purpose: TransferPurpose, started: u64) -> Self {
        Transfer {
            file_id: media.file.id,
            name: media.suggested_file_name(),
            size: media.file.size,
            purpose,
            downloaded: 0,
            state: TransferState::Downloading,
            started,
            media,
        }
    }

    /// Done, cancelled or failed: it can't be paused or resumed.
    pub fn is_finished(&self) -> bool {
        matches!(
            self.state,
            TransferState::Done | TransferState::Cancelled | TransferState::Failed(_)
        )
    }

    /// How far along it is, 0-100; None when the size is unknown.
    pub fn percent(&self) -> Option<u8> {
        if self.state == TransferState::Done {
            return Some(100);
        }
        (self.size > 0).then(|| (self.downloaded.clamp(0, self.size) * 100 / self.size) as u8)
    }

    /// The progress and state, e.g. `1.0 MiB / 4.0 MiB · 25% · paused`.
    pub fn status_text(&self) -> String {
        let amount = if self.size > 0 {
            format!(
                "{} / {}",
                format_size(self.downloaded),
                format_size(self.size)
            )
        } else {
            format_size(self.downloaded)
        };
        let state = match &self.state {
            TransferState::Downloading => "downloading".to_owned(),
            TransferState::Paused => "paused".to_owned(),
            TransferState::Done => {
                return format!("{} · done", format_size(self.size.max(self.downloaded)));
            }
            TransferState::Cancelled => return "cancelled".to_owned(),
            TransferState::Failed(why) => format!("failed: {why}"),
        };
        match self.percent() {
            Some(percent) => format!("{amount} · {percent}% · {state}"),
            None => format!("{amount} · {state}"),
        }
    }
}
