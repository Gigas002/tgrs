use super::*;

#[test]
fn photos_as_they_are_and_jpeg_xl_with_the_feature_can_be_photos() {
    assert!(can_send_as_photo("/a/b.png"));
    assert!(can_send_as_photo("/a/b.JPG"));
    assert!(!can_send_as_photo("/a/b.pdf"));
    assert!(!can_send_as_photo("/a/b"));
    assert_eq!(can_send_as_photo("/a/b.jxl"), cfg!(feature = "jxl"));
    assert_eq!(can_send_as_photo("/a/B.JXL"), cfg!(feature = "jxl"));
}

#[test]
fn files_are_sent_as_they_are() {
    let dir = tempfile::tempdir().unwrap();
    let paths = vec!["/a/b.jxl".to_owned(), "/a/c.png".to_owned()];
    let (sent, problems) = prepare_in(paths.clone(), SendAs::Files, dir.path());
    assert_eq!(sent, paths);
    assert!(problems.is_empty());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[cfg(not(feature = "jxl"))]
#[test]
fn without_the_feature_jpeg_xl_is_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let paths = vec!["/a/b.jxl".to_owned()];
    let (sent, problems) = prepare_in(paths.clone(), SendAs::Photos, dir.path());
    assert_eq!(sent, paths);
    assert!(problems.is_empty());
}

#[cfg(feature = "jxl")]
mod jxl {
    use super::*;
    use image::GenericImageView;

    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/photo/fixtures")
            .join(name)
    }

    fn to_jpeg(name: &str) -> Vec<u8> {
        jxl_to_jpeg(&std::fs::read(fixture(name)).unwrap()).unwrap()
    }

    fn decode_jpeg(jpeg: &[u8]) -> image::DynamicImage {
        image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg).unwrap()
    }

    /// Whether the JPEG carries an ICC profile (an APP2 `ICC_PROFILE` segment).
    fn has_icc(jpeg: &[u8]) -> bool {
        jpeg.windows(12).any(|w| w == b"ICC_PROFILE\0")
    }

    #[test]
    fn a_jpeg_xl_becomes_a_jpeg_of_the_same_picture() {
        let jpeg = to_jpeg("grad.jxl");
        assert_eq!(&jpeg[..2], [0xFF, 0xD8], "not a JPEG");
        let got = decode_jpeg(&jpeg).to_rgb8();
        // The fixture was encoded from this PNG (red to blue, top to bottom).
        let want = image::open(fixture("grad.png")).unwrap().to_rgb8();
        assert_eq!(got.dimensions(), want.dimensions());
        let diff: u64 = got
            .as_raw()
            .iter()
            .zip(want.as_raw())
            .map(|(a, b)| u64::from(a.abs_diff(*b)))
            .sum();
        let mean = diff as f64 / got.as_raw().len() as f64;
        assert!(mean < 4.0, "mean difference {mean}");
        // sRGB needs no profile.
        assert!(!has_icc(&jpeg));
    }

    #[test]
    fn transparency_becomes_white() {
        let image = decode_jpeg(&to_jpeg("alpha.jxl")).to_rgb8();
        let corner = image.get_pixel(0, 0).0;
        assert!(corner.iter().all(|&c| c > 245), "corner {corner:?}");
        // The middle is a half-transparent green circle: green over white.
        let [r, g, b] = image.get_pixel(16, 16).0;
        assert!(
            g > 180 && r < 200 && b < 200 && g > r,
            "middle {:?}",
            [r, g, b]
        );
    }

    #[test]
    fn grayscale_images_convert() {
        let image = decode_jpeg(&to_jpeg("gray.jxl"));
        assert_eq!(image.dimensions(), (24, 16));
        let top = image.to_luma8().get_pixel(12, 0).0[0];
        let bottom = image.to_luma8().get_pixel(12, 15).0[0];
        assert!(top > 200 && bottom < 55, "top {top}, bottom {bottom}");
    }

    #[test]
    fn a_wide_gamut_image_keeps_its_color_profile() {
        let jpeg = to_jpeg("p3.jxl");
        assert!(
            has_icc(&jpeg),
            "Display P3 without its profile shifts colors"
        );
        assert_eq!(decode_jpeg(&jpeg).dimensions(), (64, 40));
    }

    #[test]
    fn large_images_are_scaled_to_what_telegram_keeps() {
        // 2600 × 400: 400 · 2560 / 2600 = 393.8
        let image = decode_jpeg(&to_jpeg("big.jxl"));
        assert_eq!(image.dimensions(), (MAX_SIDE, 393));
    }

    #[test]
    fn an_animation_sends_its_first_frame() {
        // Two 16 × 16 frames: red, then blue.
        let image = decode_jpeg(&to_jpeg("anim.jxl")).to_rgb8();
        assert_eq!(image.dimensions(), (16, 16));
        let [r, _, b] = image.get_pixel(8, 8).0;
        assert!(r > 200 && b < 60, "first frame is red, got {:?}", [r, b]);
    }

    #[test]
    fn broken_files_are_errors_not_panics() {
        let bytes = std::fs::read(fixture("grad.jxl")).unwrap();
        let cut = jxl_to_jpeg(&bytes[..bytes.len() / 2]).unwrap_err();
        assert!(cut.contains("cut short") || cut.contains("valid"), "{cut}");
        for junk in [&b""[..], b"\xff\x0a", b"not an image at all"] {
            assert!(jxl_to_jpeg(junk).is_err(), "{junk:?}");
        }
    }

    #[test]
    fn photos_are_converted_and_other_files_left() {
        let dir = tempfile::tempdir().unwrap();
        let jxl = fixture("grad.jxl").display().to_string();
        let png = fixture("grad.png").display().to_string();
        let (sent, problems) =
            prepare_in(vec![jxl.clone(), png.clone()], SendAs::Photos, dir.path());
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(sent[1], png);
        let jpeg = Path::new(&sent[0]);
        assert_eq!(jpeg.parent(), Some(dir.path()));
        assert!(
            jpeg.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("grad-")
        );
        assert!(client::is_photo(jpeg));
        decode_jpeg(&std::fs::read(jpeg).unwrap());
    }

    #[test]
    fn a_jpeg_xl_that_fails_is_sent_as_a_file_and_said_why() {
        let dir = tempfile::tempdir().unwrap();
        let broken = dir.path().join("broken.jxl");
        std::fs::write(&broken, b"\xff\x0a not really").unwrap();
        let missing = dir.path().join("gone.jxl");
        let paths = vec![broken.display().to_string(), missing.display().to_string()];
        let (sent, problems) = prepare_in(paths.clone(), SendAs::Photos, dir.path());
        assert_eq!(sent, paths);
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(problems[0].starts_with("broken.jxl is sent as a file: "));
        assert!(problems[1].starts_with("gone.jxl is sent as a file: can't read it"));
    }
}
