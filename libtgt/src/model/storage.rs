//! How much disk space Telegram's files and database take, to free some.

use super::message::format_size;
use tdlib_rs::types::{StorageStatistics, StorageStatisticsFast};

/// The files kept for one chat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatStorage {
    /// 0 for files that belong to no chat.
    pub chat_id: i64,
    /// The chat's title; empty when unknown.
    pub title: String,
    /// Bytes.
    pub size: i64,
    pub count: i32,
}

impl ChatStorage {
    /// The title, or what stands for it, e.g. `Other files`.
    pub fn name(&self) -> String {
        match (self.chat_id, self.title.is_empty()) {
            (0, _) => "Other files".to_owned(),
            (_, false) => self.title.clone(),
            (id, true) => format!("Chat {id}"),
        }
    }

    /// E.g. `Rust · 12.0 MiB · 30 files`.
    pub fn summary(&self) -> String {
        format!(
            "{} · {} · {}",
            self.name(),
            format_size(self.size),
            files(self.count)
        )
    }
}

/// The disk space used, as the storage popup shows it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StorageUsage {
    /// Bytes of downloaded and cached files.
    pub files_size: i64,
    pub file_count: i32,
    /// Bytes of the message database.
    pub database_size: i64,
    /// Bytes of TDLib's own log.
    pub log_size: i64,
    /// The chats with the most files, biggest first.
    pub chats: Vec<ChatStorage>,
}

impl StorageUsage {
    /// From TDLib's exact statistics for the files and its fast ones for the
    /// database; `title` names a chat.
    pub fn new(
        fast: &StorageStatisticsFast,
        stats: &StorageStatistics,
        title: impl Fn(i64) -> Option<String>,
    ) -> Self {
        let mut chats: Vec<ChatStorage> = stats
            .by_chat
            .iter()
            .filter(|chat| chat.size > 0)
            .map(|chat| ChatStorage {
                chat_id: chat.chat_id,
                title: title(chat.chat_id).unwrap_or_default(),
                size: chat.size,
                count: chat.count,
            })
            .collect();
        chats.sort_by_key(|chat| std::cmp::Reverse(chat.size));
        StorageUsage {
            files_size: stats.size,
            file_count: stats.count,
            database_size: fast.database_size + fast.language_pack_database_size,
            log_size: fast.log_size,
            chats,
        }
    }

    /// Everything on disk, in bytes.
    pub fn total(&self) -> i64 {
        self.files_size + self.database_size + self.log_size
    }

    /// E.g. `Files 120.0 MiB (300 files) · Database 20.0 MiB · Log 1.0 MiB`.
    pub fn summary(&self) -> String {
        format!(
            "Files {} ({}) · Database {} · Log {}",
            format_size(self.files_size),
            files(self.file_count),
            format_size(self.database_size),
            format_size(self.log_size)
        )
    }
}

fn files(count: i32) -> String {
    if count == 1 {
        "1 file".to_owned()
    } else {
        format!("{count} files")
    }
}
