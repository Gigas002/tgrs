use std::io;

use tokio::sync::mpsc::error::SendError;

use crate::settings::SettingsError;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Io(#[from] io::Error),

    #[error(transparent)]
    Settings(#[from] SettingsError),

    /// The receiving side of a channel is gone, i.e. the app is shutting down.
    #[error("channel closed")]
    ChannelClosed,

    #[error("invalid action: {0}")]
    InvalidAction(String),

    #[error("invalid key: {0}")]
    InvalidEvent(String),

    #[error("{0}")]
    Command(String),
}

impl<T> From<SendError<T>> for AppError {
    fn from(_: SendError<T>) -> Self {
        Self::ChannelClosed
    }
}
