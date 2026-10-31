use tdlib_rs::enums::StickerFormat;
use tdlib_rs::types::{Sticker, StickerSet, StickerSetInfo};

use super::message::{MediaFile, still_thumbnail};

/// A sticker that can be sent.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StickerEntry {
    pub id: i64,
    /// The sticker's file, sent by its id.
    pub file_id: i32,
    pub width: i32,
    pub height: i32,
    /// The emoji the sticker stands for.
    pub emoji: String,
    /// A still image to show for it: its thumbnail, or a WEBP sticker itself.
    pub preview: Option<MediaFile>,
}

impl From<&Sticker> for StickerEntry {
    fn from(sticker: &Sticker) -> Self {
        let preview = still_thumbnail(sticker.thumbnail.as_ref()).or_else(|| {
            matches!(sticker.format, StickerFormat::Webp).then(|| MediaFile::from(&sticker.sticker))
        });
        Self {
            id: sticker.id,
            file_id: sticker.sticker.id,
            width: sticker.width,
            height: sticker.height,
            emoji: sticker.emoji.clone(),
            preview,
        }
    }
}

/// A set of stickers, or the recently sent ones.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StickerSetEntry {
    /// [`Self::RECENT`] for the recently sent stickers.
    pub id: i64,
    pub title: String,
    /// Empty until the set is loaded (see [`Self::is_loaded`]).
    pub stickers: Vec<StickerEntry>,
    /// How many stickers the set has.
    pub size: usize,
}

impl StickerSetEntry {
    /// The id of the recently sent stickers, which aren't a real set.
    pub const RECENT: i64 = 0;

    /// The recently sent stickers.
    pub fn recent(stickers: Vec<StickerEntry>) -> Self {
        Self {
            id: Self::RECENT,
            title: "Recent".to_owned(),
            size: stickers.len(),
            stickers,
        }
    }

    pub fn is_loaded(&self) -> bool {
        self.stickers.len() >= self.size
    }
}

/// An installed set, without its stickers.
impl From<&StickerSetInfo> for StickerSetEntry {
    fn from(set: &StickerSetInfo) -> Self {
        Self {
            id: set.id,
            title: set.title.clone(),
            stickers: Vec::new(),
            size: usize::try_from(set.size).unwrap_or(0),
        }
    }
}

impl From<&StickerSet> for StickerSetEntry {
    fn from(set: &StickerSet) -> Self {
        Self {
            id: set.id,
            title: set.title.clone(),
            stickers: set.stickers.iter().map(StickerEntry::from).collect(),
            size: set.stickers.len(),
        }
    }
}
