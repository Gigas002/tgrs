//! Images Telegram doesn't take as photos, made into ones it does before
//! sending: a JPEG XL becomes a JPEG, as Telegram Desktop does.

use std::path::{Path, PathBuf};

use libtgrs::client::{self, SendAs};

#[cfg(test)]
mod tests;

/// The longest side Telegram keeps of a photo; larger ones are scaled down
/// first, which also keeps them under its 10 MB and 10000 px limits.
#[cfg(feature = "jxl")]
const MAX_SIDE: u32 = 2560;

/// JPEG quality of converted photos; Telegram compresses them again.
#[cfg(feature = "jxl")]
const JPEG_QUALITY: u8 = 90;

/// Whether `path` can be sent as a photo: as it is, or converted.
pub fn can_send_as_photo(path: impl AsRef<Path>) -> bool {
    client::is_photo(&path) || is_convertible(path.as_ref())
}

/// A JPEG XL, which tgrs converts when built with the `jxl` feature.
fn is_convertible(path: &Path) -> bool {
    cfg!(feature = "jxl")
        && path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("jxl"))
}

/// `paths` ready to send `send_as`: with [`SendAs::Photos`], each image that
/// needs it is replaced by a converted copy in the temporary directory. One
/// that can't be converted is sent as a file; the second list says why.
/// Slow for large images: call it off the async runtime.
pub fn prepare(paths: Vec<String>, send_as: SendAs) -> (Vec<String>, Vec<String>) {
    prepare_in(paths, send_as, &std::env::temp_dir().join("tgrs-photos"))
}

/// [`prepare`], with converted photos saved in `dir`.
fn prepare_in(paths: Vec<String>, send_as: SendAs, dir: &Path) -> (Vec<String>, Vec<String>) {
    let mut problems = Vec::new();
    if send_as != SendAs::Photos {
        return (paths, problems);
    }
    let paths = paths
        .into_iter()
        .map(|path| {
            if !is_convertible(Path::new(&path)) {
                return path;
            }
            match convert(Path::new(&path), dir) {
                Ok(jpeg) => jpeg.display().to_string(),
                Err(e) => {
                    let name = Path::new(&path)
                        .file_name()
                        .map_or_else(|| path.clone(), |n| n.to_string_lossy().into_owned());
                    problems.push(format!("{name} is sent as a file: {e}"));
                    path
                }
            }
        })
        .collect();
    (paths, problems)
}

/// A JPEG of the JPEG XL at `path`, saved in `dir`.
#[cfg(feature = "jxl")]
fn convert(path: &Path, dir: &Path) -> Result<PathBuf, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("can't read it: {e}"))?;
    let jpeg = jxl_to_jpeg(&bytes)?;
    std::fs::create_dir_all(dir).map_err(|e| format!("can't save the photo: {e}"))?;
    let stem = path
        .file_stem()
        .map_or_else(|| "photo".into(), |s| s.to_string_lossy());
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let out = dir.join(format!("{stem}-{stamp}.jpg"));
    std::fs::write(&out, jpeg).map_err(|e| format!("can't save the photo: {e}"))?;
    Ok(out)
}

#[cfg(not(feature = "jxl"))]
fn convert(_: &Path, _: &Path) -> Result<PathBuf, String> {
    Err("tgrs is built without the jxl feature".to_owned())
}

/// The first frame of a JPEG XL as a JPEG: oriented, flattened onto white,
/// scaled to 2560 pixels on the long side, with the image's color profile unless it's sRGB.
#[cfg(feature = "jxl")]
pub fn jxl_to_jpeg(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let decoded = decode_jxl(bytes)?;
    let rgba = image::RgbaImage::from_raw(decoded.width, decoded.height, decoded.rgba)
        .ok_or("the decoded image has the wrong size")?;
    let rgba = fit(rgba, MAX_SIDE);
    let rgb = on_white(&rgba);
    let mut jpeg = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, JPEG_QUALITY);
    if let Some(icc) = decoded.icc {
        use image::ImageEncoder;
        // An encoder without ICC support only loses the profile.
        let _ = encoder.set_icc_profile(icc);
    }
    encoder
        .encode(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|e| format!("can't encode it as a JPEG: {e}"))?;
    Ok(jpeg)
}

/// Decoded pixels: 8-bit RGBA rows, in the color space of `icc`, or sRGB.
#[cfg(feature = "jxl")]
struct Decoded {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    icc: Option<Vec<u8>>,
}

/// At most this many samples (pixels × channels) are decoded: 64 MP of RGBA.
#[cfg(feature = "jxl")]
const SAMPLE_LIMIT: usize = 1 << 28;

#[cfg(feature = "jxl")]
fn decode_jxl(mut input: &[u8]) -> Result<Decoded, String> {
    use jxl::api::{
        Event, JxlColorEncoding, JxlColorProfile, JxlDecoder, JxlDecoderOptions, JxlOutputBuffer,
        JxlPixelFormat,
    };

    let invalid = |e: jxl::error::Error| format!("not a valid JPEG XL image ({e})");
    let mut options = JxlDecoderOptions::default();
    options.sample_limit = Some(SAMPLE_LIMIT);
    let mut decoder = JxlDecoder::new(options);
    let mut pixels: Option<(usize, usize, Vec<u8>)> = None;
    loop {
        let mut buffers = pixels
            .as_mut()
            .map(|(width, height, rgba)| [JxlOutputBuffer::new(rgba, *height, *width * 4)]);
        let event = decoder
            .process(&mut input, buffers.as_mut().map(|b| b.as_mut_slice()), None)
            .map_err(invalid)?;
        match event {
            Event::BasicInfo => {
                let info = decoder.basic_info().ok_or("no image header")?;
                let (width, height) = info.size;
                let extra_channels = info.extra_channels.len();
                decoder
                    .set_pixel_format(JxlPixelFormat::rgba8(extra_channels))
                    .map_err(invalid)?;
                let len = width
                    .checked_mul(height)
                    .and_then(|n| n.checked_mul(4))
                    .ok_or("the image is too large")?;
                pixels = Some((width, height, vec![0; len]));
            }
            Event::FrameHeader => {}
            // The first frame is the photo; an animation's others are left.
            Event::FrameComplete { .. } | Event::Complete => break,
            Event::NeedMoreInput { .. } => return Err("the file is cut short".to_owned()),
        }
    }
    let (width, height, rgba) = pixels.ok_or("no image in the file")?;
    let srgb = JxlColorProfile::Simple(JxlColorEncoding::srgb(false));
    let icc = decoder
        .output_color_profile()
        .filter(|profile| !profile.same_color_encoding(&srgb))
        .and_then(|profile| profile.try_as_icc())
        .map(|icc| icc.into_owned());
    Ok(Decoded {
        width: u32::try_from(width).map_err(|_| "the image is too wide")?,
        height: u32::try_from(height).map_err(|_| "the image is too tall")?,
        rgba,
        icc,
    })
}

/// `image` scaled down so neither side exceeds `max_side`, keeping its shape.
#[cfg(feature = "jxl")]
fn fit(image: image::RgbaImage, max_side: u32) -> image::RgbaImage {
    let (width, height) = image.dimensions();
    let longest = width.max(height);
    if longest <= max_side {
        return image;
    }
    let scale = |side: u32| {
        u32::try_from(u64::from(side) * u64::from(max_side) / u64::from(longest))
            .unwrap_or(max_side)
            .max(1)
    };
    image::imageops::thumbnail(&image, scale(width), scale(height))
}

/// `image` over a white background: JPEG has no transparency.
#[cfg(feature = "jxl")]
fn on_white(image: &image::RgbaImage) -> image::RgbImage {
    image::RgbImage::from_fn(image.width(), image.height(), |x, y| {
        let [r, g, b, a] = image.get_pixel(x, y).0;
        let over = |c: u8| {
            let (c, a) = (u16::from(c), u16::from(a));
            // c·a + 255·(1 − a), rounded, all over 255
            u8::try_from((c * a + 255 * (255 - a) + 127) / 255).unwrap_or(u8::MAX)
        };
        image::Rgb([over(r), over(g), over(b)])
    })
}
