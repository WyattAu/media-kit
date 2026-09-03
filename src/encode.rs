//! Output encoding for supported formats.
//!
//! # WebP quality
//!
//! `image` 0.25 can only encode **lossless** WebP, so `OutFormat::WebP(Some(q))`
//! silently ignores the quality value unless the `webp-lossy` feature is
//! enabled, in which case true lossy encoding is performed via libwebp
//! (through the [`webp`] crate).

use image::DynamicImage;

use crate::error::MediaError;

/// Output container/codec.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum OutFormat {
    /// JPEG with the given quality (1–100, image crate clamps internally).
    Jpeg(u8),
    /// PNG (lossless; quality is not applicable).
    Png,
    /// GIF (palette-based, lossless-ish).
    Gif,
    /// WebP. `None` = lossless; `Some(q)` = lossy quality 0.0–100.0.
    /// Lossy is only real with the `webp-lossy` feature (see module docs).
    WebP(Option<f32>),
}

impl OutFormat {
    /// Canonical file extension, e.g. `"jpg"`.
    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            OutFormat::Jpeg(_) => "jpg",
            OutFormat::Png => "png",
            OutFormat::Gif => "gif",
            OutFormat::WebP(_) => "webp",
        }
    }

    /// MIME type, e.g. `"image/jpeg"`.
    #[must_use]
    pub fn mime(self) -> &'static str {
        match self {
            OutFormat::Jpeg(_) => "image/jpeg",
            OutFormat::Png => "image/png",
            OutFormat::Gif => "image/gif",
            OutFormat::WebP(_) => "image/webp",
        }
    }
}

/// Encode `img` to `fmt`, returning the encoded bytes.
///
/// # Errors
///
/// [`MediaError::Encode`] when the encoder rejects the image or a
/// format-specific feature is compiled out.
pub fn encode(img: &DynamicImage, fmt: &OutFormat) -> Result<Vec<u8>, MediaError> {
    match fmt {
        OutFormat::Jpeg(quality) => encode_jpeg(img, *quality),
        OutFormat::Png => write_via_image(img, image::ImageFormat::Png),
        OutFormat::Gif => write_via_image(img, image::ImageFormat::Gif),
        OutFormat::WebP(quality) => encode_webp(img, *quality),
    }
}

fn encode_jpeg(img: &DynamicImage, quality: u8) -> Result<Vec<u8>, MediaError> {
    #[cfg(feature = "jpeg")]
    {
        let rgb = img.to_rgb8();
        let mut out = Vec::new();
        let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality);
        rgb.write_with_encoder(enc)
            .map_err(|e| MediaError::Encode(e.to_string()))?;
        Ok(out)
    }
    #[cfg(not(feature = "jpeg"))]
    {
        let _ = (img, quality);
        Err(MediaError::Encode(
            "jpeg feature disabled; enable feature \"jpeg\"".into(),
        ))
    }
}

fn write_via_image(img: &DynamicImage, fmt: image::ImageFormat) -> Result<Vec<u8>, MediaError> {
    // Each arm gates on its own crate feature, so a single `matches!` is wrong.
    #[allow(clippy::match_like_matches_macro)]
    let enabled = match fmt {
        image::ImageFormat::Png => cfg!(feature = "png"),
        image::ImageFormat::Gif => cfg!(feature = "gif"),
        image::ImageFormat::WebP => cfg!(feature = "webp"),
        _ => false,
    };
    if !enabled {
        return Err(MediaError::Encode(format!(
            "{fmt:?} feature disabled; enable the matching feature"
        )));
    }
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), fmt)
        .map_err(|e| MediaError::Encode(e.to_string()))?;
    Ok(out)
}

fn encode_webp(img: &DynamicImage, quality: Option<f32>) -> Result<Vec<u8>, MediaError> {
    #[cfg(feature = "webp-lossy")]
    {
        if let Some(q) = quality {
            return encode_webp_lossy(img, q);
        }
        write_via_image(img, image::ImageFormat::WebP)
    }
    #[cfg(not(feature = "webp-lossy"))]
    {
        let _ = quality;
        write_via_image(img, image::ImageFormat::WebP)
    }
}

#[cfg(feature = "webp-lossy")]
fn encode_webp_lossy(img: &DynamicImage, quality: f32) -> Result<Vec<u8>, MediaError> {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let raw = rgba.into_raw();
    let encoder = webp::Encoder::from_rgba(&raw, w, h);
    let q = quality.clamp(0.0, 100.0);
    let mem = encoder.encode(q);
    Ok(mem.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;
    use image::RgbaImage;

    fn img(w: u32, h: u32) -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(w, h, image::Rgba([10, 200, 30, 255])))
    }

    #[test]
    fn jpeg_quality_roundtrip() {
        let bytes = encode(&img(8, 8), &OutFormat::Jpeg(80)).unwrap();
        assert_eq!(crate::sniff::sniff(&bytes), Some(crate::sniff::Format::Jpeg));
        let back = image::load_from_memory(&bytes).unwrap();
        assert_eq!((back.width(), back.height()), (8, 8));
    }

    #[test]
    fn png_roundtrip() {
        let bytes = encode(&img(4, 4), &OutFormat::Png).unwrap();
        assert_eq!(crate::sniff::sniff(&bytes), Some(crate::sniff::Format::Png));
    }

    #[test]
    fn gif_roundtrip() {
        let bytes = encode(&img(4, 4), &OutFormat::Gif).unwrap();
        assert_eq!(crate::sniff::sniff(&bytes), Some(crate::sniff::Format::Gif));
    }

    #[test]
    fn webp_roundtrip() {
        let bytes = encode(&img(4, 4), &OutFormat::WebP(None)).unwrap();
        assert_eq!(crate::sniff::sniff(&bytes), Some(crate::sniff::Format::WebP));
    }

    #[test]
    fn webp_lossy_decodes() {
        // With webp-lossy this exercises libwebp; without it the quality is
        // ignored and lossless encoding is used. Both must decode.
        let bytes = encode(&testutil::noise_image(16, 16), &OutFormat::WebP(Some(75.0))).unwrap();
        let back = image::load_from_memory_with_format(&bytes, image::ImageFormat::WebP).unwrap();
        assert_eq!((back.width(), back.height()), (16, 16));
    }

    #[test]
    fn extensions_and_mimes() {
        assert_eq!(OutFormat::Jpeg(90).extension(), "jpg");
        assert_eq!(OutFormat::Png.mime(), "image/png");
        assert_eq!(OutFormat::WebP(None).mime(), "image/webp");
    }

    #[test]
    fn jpeg_quality_changes_output() {
        let noisy = testutil::noise_image(64, 64);
        let hi = encode(&noisy, &OutFormat::Jpeg(95)).unwrap();
        let lo = encode(&noisy, &OutFormat::Jpeg(10)).unwrap();
        assert!(hi.len() > lo.len(), "hi {} vs lo {}", hi.len(), lo.len());
    }
}
