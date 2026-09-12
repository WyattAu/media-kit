// Test fixtures: in-memory encodes of valid images have no failure path
// (see INVARIANT comments); expect keeps violations loud.
#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Shared sample media for doctests, unit tests, and benches.
//!
//! Encoded samples are generated once per process and cached.

#[cfg(any(feature = "jpeg", feature = "png"))]
use std::io::Cursor;
#[cfg(any(feature = "jpeg", feature = "png"))]
use std::sync::OnceLock;

use image::DynamicImage;
use image::RgbaImage;

#[cfg(feature = "jpeg")]
static TINY_JPEG: OnceLock<Vec<u8>> = OnceLock::new();

/// Deterministic 8x8 JPEG.
#[cfg(feature = "jpeg")]
#[must_use]
#[allow(clippy::expect_used)] // in-memory encode of a valid image cannot fail
pub fn tiny_jpeg() -> Vec<u8> {
    TINY_JPEG
        .get_or_init(|| {
            let img = DynamicImage::ImageRgba8(RgbaImage::from_fn(8, 8, |x, y| {
                image::Rgba([
                    ((x * 31 + y * 17) % 256) as u8,
                    128,
                    ((x * y) % 256) as u8,
                    255,
                ])
            }));
            let rgb = img.to_rgb8();
            let mut out = Vec::new();
            let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(Cursor::new(&mut out), 90);
            // INVARIANT: encoding a fully-constructed in-memory RGB8 buffer
            // cannot fail — the only error sources are I/O (none: Vec cursor)
            // and malformed image state (impossible here by construction).
            rgb.write_with_encoder(enc)
                .expect("INVARIANT: in-memory JPEG encode of a valid RGB8 image cannot fail");
            out
        })
        .clone()
}

/// Deterministic PNG of arbitrary size.
#[cfg(feature = "png")]
#[must_use]
#[allow(clippy::expect_used)] // in-memory encode of a valid image cannot fail
pub fn tiny_png(w: u32, h: u32) -> Vec<u8> {
    let img = DynamicImage::ImageRgba8(RgbaImage::from_fn(w, h, |x, y| {
        image::Rgba([
            ((x * 13 + y * 7) % 256) as u8,
            ((x + y) % 256) as u8,
            200,
            255,
        ])
    }));
    let mut out = Vec::new();
    // INVARIANT: same as above — Vec cursor + a validly-constructed image
    // leaves the PNG encoder no failure path.
    img.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
        .expect("INVARIANT: in-memory PNG encode of a valid RGBA8 image cannot fail");
    out
}

/// Deterministic pseudo-noise RGB image (exercises encoders better than
/// flat colors).
#[must_use]
pub fn noise_image(w: u32, h: u32) -> DynamicImage {
    DynamicImage::ImageRgba8(RgbaImage::from_fn(w, h, |x, y| {
        let r = (x
            .wrapping_mul(2654435761)
            .wrapping_add(y.wrapping_mul(40503)))
            % 251;
        let g = (x
            .wrapping_mul(2246822519)
            .wrapping_add(y.wrapping_mul(3266489917)))
            % 253;
        let b = (x
            .wrapping_mul(668265263)
            .wrapping_add(y.wrapping_mul(374761393)))
            % 249;
        image::Rgba([r as u8, g as u8, b as u8, 255])
    }))
}

/// Build the raw little-endian TIFF block for a minimal EXIF IFD containing
/// exactly one `Orientation` (tag 0x0112, SHORT) entry with value `v`.
#[cfg(all(feature = "jpeg", feature = "exif"))]
#[must_use]
pub fn orientation_tiff(v: u16) -> Vec<u8> {
    let mut t = Vec::with_capacity(26);
    t.extend_from_slice(b"II\x2A\x00"); // little-endian, magic 42
    t.extend_from_slice(&8u32.to_le_bytes()); // IFD0 offset
    t.extend_from_slice(&1u16.to_le_bytes()); // one entry
    t.extend_from_slice(&0x0112u16.to_le_bytes()); // Orientation tag
    t.extend_from_slice(&3u16.to_le_bytes()); // type SHORT
    t.extend_from_slice(&1u32.to_le_bytes()); // count
    t.extend_from_slice(&v.to_le_bytes()); // value (SHORT, first 2 bytes)
    t.extend_from_slice(&[0, 0]); // value padding
    t.extend_from_slice(&0u32.to_le_bytes()); // no next IFD
    t
}

/// Wrap a raw TIFF block as a JPEG APP1 EXIF segment (marker, BE length,
/// `Exif\0\0` signature, TIFF payload).
#[cfg(all(feature = "jpeg", feature = "exif"))]
#[must_use]
pub fn exif_app1_segment(tiff: &[u8]) -> Vec<u8> {
    let payload = 6 + tiff.len();
    let mut seg = Vec::with_capacity(4 + payload);
    seg.extend_from_slice(&[0xFF, 0xE1]);
    seg.extend_from_slice(
        &u16::try_from(payload + 2)
            .expect("segment < 64 KiB")
            .to_be_bytes(),
    );
    seg.extend_from_slice(b"Exif\0\0");
    seg.extend_from_slice(tiff);
    seg
}

/// Deterministic JPEG carrying a crafted EXIF orientation tag `v` (1–8).
/// The APP1 segment is spliced right after the SOI of [`tiny_jpeg`].
#[cfg(all(feature = "jpeg", feature = "exif"))]
#[must_use]
pub fn jpeg_with_orientation(v: u16) -> Vec<u8> {
    let jpg = tiny_jpeg();
    let app1 = exif_app1_segment(&orientation_tiff(v));
    let mut out = Vec::with_capacity(2 + app1.len() + jpg.len() - 2);
    out.extend_from_slice(&jpg[..2]); // SOI
    out.extend_from_slice(&app1);
    out.extend_from_slice(&jpg[2..]); // remaining segments + scan
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(any(feature = "jpeg", feature = "png"))]
    use crate::sniff;

    #[test]
    #[cfg(feature = "jpeg")]
    fn tiny_jpeg_sniffs_as_jpeg() {
        assert_eq!(sniff::sniff(&tiny_jpeg()), Some(sniff::Format::Jpeg));
    }

    #[test]
    #[cfg(feature = "png")]
    fn tiny_png_sniffs_as_png() {
        assert_eq!(sniff::sniff(&tiny_png(4, 4)), Some(sniff::Format::Png));
    }

    #[test]
    #[cfg(feature = "jpeg")]
    fn tiny_jpeg_dims() {
        let img = image::load_from_memory(&tiny_jpeg()).unwrap();
        assert_eq!((img.width(), img.height()), (8, 8));
    }

    #[test]
    fn noise_is_noisy() {
        let img = noise_image(8, 8).to_rgb8();
        let first = img.get_pixel(0, 0).0;
        let second = img.get_pixel(1, 0).0;
        assert_ne!(first, second);
    }
}
