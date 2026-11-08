use super::*;
use crate::ui::tests::create_test_app_context;
use tokio::sync::mpsc::UnboundedReceiver;

fn sticker(id: i64, emoji: &str) -> StickerEntry {
    StickerEntry {
        id,
        file_id: id as i32,
        emoji: emoji.to_owned(),
        ..StickerEntry::default()
    }
}

fn set(id: i64, title: &str, stickers: Vec<StickerEntry>) -> StickerSetEntry {
    StickerSetEntry {
        id,
        title: title.to_owned(),
        size: stickers.len(),
        stickers,
    }
}

/// An installed set whose stickers aren't loaded yet.
fn unloaded(id: i64, title: &str, size: usize) -> StickerSetEntry {
    StickerSetEntry {
        id,
        title: title.to_owned(),
        stickers: Vec::new(),
        size,
    }
}

fn create_picker() -> (StickerPicker, UnboundedReceiver<Action>) {
    let mut picker = StickerPicker::new(create_test_app_context());
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    picker.register_action_handler(tx).unwrap();
    (picker, rx)
}

fn sent(rx: &mut UnboundedReceiver<Action>) -> Vec<Action> {
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

/// Opened, with the recent stickers 😀 😂 and an unloaded set of cats.
fn opened() -> (StickerPicker, UnboundedReceiver<Action>) {
    let (mut picker, mut rx) = create_picker();
    picker.update(Action::ShowStickerPicker);
    picker.update(Action::StickerSetsLoaded(vec![
        StickerSetEntry::recent(vec![sticker(1, "😀"), sticker(2, "😂")]),
        unloaded(7, "Cats", 3),
    ]));
    assert_eq!(sent(&mut rx), [Action::LoadStickerSets]);
    (picker, rx)
}

#[test]
fn sets_are_loaded_when_shown() {
    let (mut picker, mut rx) = opened();
    picker.update(Action::StickerPickerNextSet);
    picker.update(Action::StickerPickerPreviousSet);
    picker.update(Action::StickerPickerNextSet);
    // Asked for once
    assert_eq!(sent(&mut rx), [Action::LoadStickerSet(7)]);
    assert!(picker.selected_sticker().is_none());

    picker.update(Action::StickerSetLoaded(set(
        7,
        "Cats",
        vec![sticker(10, "🐱"), sticker(11, "😺"), sticker(12, "😸")],
    )));
    assert_eq!(picker.selected_sticker().map(|s| s.id), Some(10));
}

#[test]
fn reopening_keeps_loaded_sets_but_not_the_recent_stickers() {
    let (mut picker, mut rx) = opened();
    picker.update(Action::StickerSetLoaded(set(
        7,
        "Cats",
        vec![sticker(10, "🐱")],
    )));
    picker.update(Action::ShowStickerPicker);
    picker.update(Action::StickerSetsLoaded(vec![
        StickerSetEntry::recent(vec![sticker(10, "🐱")]),
        unloaded(7, "Cats", 1),
    ]));
    let sets = picker.sets().unwrap();
    assert_eq!(sets[0].stickers, [sticker(10, "🐱")]);
    assert_eq!(sets[1].stickers, [sticker(10, "🐱")]);
    assert_eq!(sent(&mut rx), [Action::LoadStickerSets]);
}

#[test]
fn arrows_move_through_the_grid() {
    let (mut picker, _rx) = create_picker();
    let stickers = (0..7).map(|id| sticker(id, "🙂")).collect();
    picker.update(Action::StickerSetsLoaded(vec![set(1, "Smiles", stickers)]));
    picker.columns = 3;
    let selected = |picker: &StickerPicker| picker.selected;

    picker.update(Action::StickerPickerUp);
    picker.update(Action::StickerPickerLeft);
    assert_eq!(selected(&picker), 0);
    picker.update(Action::StickerPickerRight);
    picker.update(Action::StickerPickerDown);
    assert_eq!(selected(&picker), 4);
    // The last row has only the 7th sticker
    picker.update(Action::StickerPickerDown);
    assert_eq!(selected(&picker), 6);
    picker.update(Action::StickerPickerDown);
    picker.update(Action::StickerPickerRight);
    assert_eq!(selected(&picker), 6);
    picker.update(Action::StickerPickerUp);
    assert_eq!(selected(&picker), 3);
}

#[test]
fn sending_closes_the_picker() {
    let (mut picker, mut rx) = opened();
    picker.update(Action::StickerPickerRight);
    picker.update(Action::StickerPickerSend);
    assert_eq!(
        sent(&mut rx),
        [
            Action::HideStickerPicker,
            Action::SendSticker(sticker(2, "😂"))
        ]
    );
}

#[test]
fn draws_the_sets_and_the_stickers_emoji() {
    use ratatui::{Terminal, backend::TestBackend};
    let (mut picker, _rx) = opened();
    let (width, height) = (100, 30);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| picker.draw(frame, frame.area()).unwrap())
        .unwrap();
    let screen: String = terminal
        .backend()
        .buffer()
        .content()
        .chunks(usize::from(width))
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>() + "\n")
        .collect();
    assert!(screen.contains(" Stickers "), "{screen}");
    assert!(screen.contains("Recent"), "{screen}");
    assert!(screen.contains("Cats"), "{screen}");
    assert!(screen.contains("😂"), "{screen}");
    assert!(screen.contains("1/2"), "{screen}");
    // Six stickers fit in a row
    assert_eq!(picker.columns, 6);
}

#[cfg(feature = "media-preview")]
#[test]
fn sticker_images_are_requested_and_drawn_in_the_picker() {
    use crate::app::action::{PreviewDecodedPayload, PreviewRequest};
    use libtgrs::model::MediaFile;
    use ratatui::{Terminal, backend::TestBackend, style::Color};

    let preview = MediaFile {
        id: 55,
        ..MediaFile::default()
    };
    let entry = StickerEntry {
        preview: Some(preview),
        preview_fallback: Some(MediaFile {
            id: 56,
            ..MediaFile::default()
        }),
        emoji: "🐱".to_owned(),
        ..StickerEntry::default()
    };
    let (mut picker, mut rx) = create_picker();
    picker.update(Action::StickerSetsLoaded(vec![StickerSetEntry::recent(
        vec![entry],
    )]));

    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal
        .draw(|frame| picker.draw(frame, frame.area()).unwrap())
        .unwrap();
    assert_eq!(
        sent(&mut rx),
        [Action::LoadPreview(PreviewRequest {
            message_id: 0,
            file_id: 55,
            path: None,
        })]
    );

    picker.update(Action::PreviewDecoded(PreviewDecodedPayload {
        request: PreviewRequest {
            message_id: 0,
            file_id: 55,
            path: None,
        },
        result: Err("thumbnail unavailable".to_owned()),
    }));
    terminal
        .draw(|frame| picker.draw(frame, frame.area()).unwrap())
        .unwrap();
    assert_eq!(
        sent(&mut rx),
        [Action::LoadPreview(PreviewRequest {
            message_id: 0,
            file_id: 56,
            path: None,
        })]
    );

    picker.update(Action::PreviewDecoded(PreviewDecodedPayload {
        request: PreviewRequest {
            message_id: 0,
            file_id: 56,
            path: None,
        },
        result: Ok(image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            32,
            32,
            image::Rgb([10, 200, 10]),
        ))),
    }));
    terminal
        .draw(|frame| picker.draw(frame, frame.area()).unwrap())
        .unwrap();
    assert!(
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .any(|cell| cell.fg == Color::Rgb(10, 200, 10) || cell.bg == Color::Rgb(10, 200, 10)),
        "the decoded sticker image is drawn"
    );
}
