//! Open-chat message cache and view window.
//! UI reads it only through the read-only API.

use std::collections::{BTreeMap, HashSet};

use crate::model::message::MessageEntry;

/// View window: the loaded range of message IDs (oldest = earliest, newest = latest).
#[derive(Debug, Default, Clone)]
pub struct LoadedRange {
    /// Oldest (earliest) message ID in the loaded buffer; None if empty.
    pub oldest_message_id: Option<i64>,
    /// Newest (latest) message ID in the loaded buffer; None if empty.
    pub newest_message_id: Option<i64>,
}

impl LoadedRange {
    pub fn new() -> Self {
        Self::default()
    }

    /// Update range with a new message id (e.g. when appending history).
    pub fn extend_with(&mut self, message_id: i64) {
        match (self.oldest_message_id, self.newest_message_id) {
            (None, None) => {
                self.oldest_message_id = Some(message_id);
                self.newest_message_id = Some(message_id);
            }
            (Some(old), Some(new)) => {
                self.oldest_message_id = Some(old.min(message_id));
                self.newest_message_id = Some(new.max(message_id));
            }
            _ => {}
        }
    }

    /// Reset after clear.
    pub fn clear(&mut self) {
        self.oldest_message_id = None;
        self.newest_message_id = None;
    }
}

/// Message cache keyed by ID with stable order (BTreeMap) and view window.
#[derive(Debug, Default)]
pub struct OpenChatMessageStore {
    cache: BTreeMap<i64, MessageEntry>,
    range: LoadedRange,
    /// Temporary ids of sent messages that got their final id: a history
    /// batch requested before the message was sent may still carry them,
    /// and they mustn't come back next to the sent message.
    replaced: HashSet<i64>,
}

/// Maximum number of messages to keep in memory per chat to prevent unbounded growth.
/// When exceeded, messages outside the sliding window are evicted based on which direction
/// the user is scrolling. This prevents performance degradation when scrolling back months.
/// Using 500 allows for jump loading (~100 older + ~300 newer) plus incremental scroll batches.
pub(crate) const MAX_MESSAGES_IN_CACHE: usize = 500;
/// When cache is full, keep this many messages and evict the rest from the opposite end
pub(crate) const MIN_MESSAGES_AFTER_EVICTION: usize = 400;

impl OpenChatMessageStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert messages (backend/task only). Updates view window.
    /// If cache exceeds MAX_MESSAGES_IN_CACHE, evicts messages based on scroll direction.
    pub fn insert_messages(&mut self, messages: impl IntoIterator<Item = MessageEntry>) {
        let old_oldest = self.range.oldest_message_id;

        for entry in messages {
            let id = entry.id();
            if self.replaced.contains(&id) {
                continue;
            }
            self.range.extend_with(id);
            self.cache.insert(id, entry);
        }

        // Evict messages if cache is too large (sliding window)
        if self.cache.len() > MAX_MESSAGES_IN_CACHE {
            let to_remove = self.cache.len() - MIN_MESSAGES_AFTER_EVICTION;

            // Detect scroll direction by comparing old and new ranges
            let scrolling_backwards = match (old_oldest, self.range.oldest_message_id) {
                (Some(old), Some(new)) => new < old, // Loading older messages
                _ => false,
            };

            let keys_to_remove: Vec<i64> = if scrolling_backwards {
                // Scrolling backwards (loading older): evict NEWEST messages
                self.cache.keys().rev().take(to_remove).copied().collect()
            } else {
                // Scrolling forwards or receiving new: evict OLDEST messages
                self.cache.keys().take(to_remove).copied().collect()
            };

            for key in &keys_to_remove {
                self.cache.remove(key);
            }

            // Recompute range after eviction
            self.recompute_range();

            tracing::debug!(
                removed = to_remove,
                remaining = self.cache.len(),
                direction = if scrolling_backwards {
                    "backwards"
                } else {
                    "forwards"
                },
                "Evicted messages from cache (sliding window)"
            );
        }
    }

    /// Clear all messages and view window (e.g. when switching chat).
    pub fn clear(&mut self) {
        self.cache.clear();
        self.range.clear();
        self.replaced.clear();
    }

    /// A sent message got its final id (or failed to send): `entry` takes the
    /// place of the message with the temporary `old_id`. Nothing is added if
    /// that message isn't loaded, e.g. after a jump elsewhere in the chat;
    /// loading the history brings the message then. Returns whether it was
    /// replaced.
    pub fn replace_message(&mut self, old_id: i64, entry: MessageEntry) -> bool {
        self.replaced.insert(old_id);
        if self.cache.remove(&old_id).is_none() {
            return false;
        }
        self.recompute_range();
        self.insert_messages(std::iter::once(entry));
        true
    }

    fn recompute_range(&mut self) {
        self.range = self.cache.keys().fold(LoadedRange::new(), |mut r, &k| {
            r.extend_with(k);
            r
        });
    }

    /// Ordered message IDs (oldest to newest by id; BTreeMap iterates in key order).
    /// UI uses this to build the list.
    pub fn ordered_message_ids(&self) -> Vec<i64> {
        self.cache.keys().copied().collect()
    }

    /// Ordered messages (oldest to newest). Single lock for a consistent snapshot; use in UI draw.
    pub fn ordered_messages(&self) -> Vec<MessageEntry> {
        self.cache.values().cloned().collect()
    }

    /// Get a message by ID (clone). UI read-only.
    pub fn get_message(&self, id: i64) -> Option<MessageEntry> {
        self.cache.get(&id).cloned()
    }

    /// Get mutable reference to a message (backend only, e.g. for editing content).
    pub fn get_message_mut(&mut self, id: i64) -> Option<&mut MessageEntry> {
        self.cache.get_mut(&id)
    }

    pub fn len(&self) -> usize {
        self.cache.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Oldest loaded message ID (read-only for UI).
    pub fn oldest_message_id(&self) -> Option<i64> {
        self.range.oldest_message_id
    }

    /// Newest loaded message ID (read-only for UI).
    pub fn newest_message_id(&self) -> Option<i64> {
        self.range.newest_message_id
    }

    /// For backend: set from_message_id for next "load older" request.
    /// Should be the current oldest message id, or 0 if empty.
    pub fn from_message_id_for_load_older(&self) -> i64 {
        self.range.oldest_message_id.unwrap_or(0)
    }

    /// Remove a single message (e.g. delete_message). Recomputes view window from remaining entries.
    pub fn remove_message(&mut self, id: i64) {
        self.cache.remove(&id);
        self.recompute_range();
    }
}
