use super::*;
use crate::app::action::{PreviewDecodedPayload, PreviewRequest};

fn file(id: i32) -> MediaFile {
    MediaFile {
        id,
        ..MediaFile::default()
    }
}

fn decoded(file_id: i32) -> PreviewDecodedPayload {
    PreviewDecodedPayload {
        request: PreviewRequest {
            message_id: 1,
            file_id,
            path: None,
        },
        result: Ok(image::DynamicImage::new_rgb8(8, 6)),
    }
}

#[test]
fn a_preview_is_requested_once() {
    let mut previews = ChatPreviews::default();
    let action = previews.request(5, &file(3));
    assert_eq!(
        action,
        Some(Action::LoadPreview(PreviewRequest {
            message_id: 5,
            file_id: 3,
            path: None,
        }))
    );
    assert_eq!(previews.request(5, &file(3)), None);
}

#[test]
fn only_requested_previews_are_kept() {
    let mut previews = ChatPreviews::default();
    previews.request(1, &file(3));
    previews.decoded(decoded(4));
    assert!(!previews.is_loaded(4), "never requested");
    previews.decoded(decoded(3));
    assert!(previews.is_loaded(3));
}

#[test]
fn the_cache_drops_the_oldest_previews() {
    let mut previews = ChatPreviews::default();
    for id in 0..40 {
        previews.request(1, &file(id));
    }
    assert_eq!(previews.cached(), CHAT_CACHE_SIZE);
    // Dropped ones are requested again when shown
    assert!(previews.request(1, &file(0)).is_some());
    assert!(previews.request(1, &file(39)).is_none());
}

#[test]
fn images_fit_their_area() {
    let area = Rect::new(0, 0, 40, 8);
    let (width, height) = fit(area, 4000, 3000);
    assert!(width <= 40 && height <= 8);
    assert!(
        width == 40 || height == 8,
        "fills one side: {width}x{height}"
    );
}
