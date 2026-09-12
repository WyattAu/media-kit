//! EXIF metadata extraction and orientation handling (behind the `exif`
//! feature).
//!
//! # Privacy
//!
//! Every encode path in this crate starts from raw pixels, so **all EXIF/GPS
//! metadata is stripped from output by default** — including GPS coordinates
//! and serial numbers embedded by cameras. This is deliberate for user
//! uploads; see [`strip_note`]. To keep metadata anyway, extract what you
//! need with [`read_exif`] and/or re-inject the original EXIF block into a
//! JPEG output with [`app1_segment`] (opt-in preservation).
//!
//! # Orientation
//!
//! Camera photos store pixels rotated with an EXIF orientation tag instead
//! of upright. [`read_orientation`] reads that tag and [`Orientation::apply`]
//! turns pixels upright (all 8 orientations). [`Pipeline auto-orient`]
//! (see [`crate::pipeline::Pipeline::auto_orient`]) applies this
//! automatically before any other op, so variants come out upright.

use crate::error::MediaError;
use image::DynamicImage;

/// Flattened EXIF fields of interest.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ExifData {
    /// Camera manufacturer (`Make`).
    pub camera_make: Option<String>,
    /// Camera model (`Model`).
    pub camera_model: Option<String>,
    /// Capture date (`DateTimeOriginal`), as a raw string.
    pub date_taken: Option<String>,
    /// GPS latitude in decimal degrees, when tagged.
    pub gps_lat: Option<f64>,
    /// GPS longitude in decimal degrees, when tagged.
    pub gps_lon: Option<f64>,
    /// Raw `Orientation` tag value 1–8, when tagged (see [`Orientation`]).
    pub orientation: Option<u8>,
}

/// Read EXIF metadata from JPEG or TIFF bytes. Returns [`None`] when the
/// container carries no EXIF; returns an error when EXIF exists but cannot
/// be parsed.
///
/// # Errors
///
/// [`MediaError::Exif`] when the reader fails to parse a present EXIF segment.
pub fn read_exif(bytes: &[u8]) -> Result<Option<ExifData>, MediaError> {
    use exif::{In, Tag};

    let mut cursor = std::io::Cursor::new(bytes);
    let reader = exif::Reader::new();
    let exif = match reader.read_from_container(&mut cursor) {
        Ok(e) => e,
        // No EXIF segment at all is not an error for our purposes.
        Err(exif::Error::NotFound(_)) => return Ok(None),
        Err(e) => return Err(MediaError::Exif(e.to_string())),
    };

    let get = |tag: Tag| {
        exif.get_field(tag, In::PRIMARY)
            .map(|f| f.display_value().to_string())
    };

    let gps_coord = |tag: Tag| -> Option<f64> {
        let field = exif.get_field(tag, In::PRIMARY)?;
        let exif::Value::Rational(rats) = &field.value else {
            return None;
        };
        if rats.len() < 3 {
            return None;
        }
        let deg = rats[0].to_f64();
        let min = rats[1].to_f64();
        let sec = rats[2].to_f64();
        let sign = if deg < 0.0 { -1.0 } else { 1.0 };
        Some(sign * (deg.abs() + min / 60.0 + sec / 3600.0))
    };

    let orientation = exif
        .get_field(Tag::Orientation, In::PRIMARY)
        .and_then(|f| match &f.value {
            exif::Value::Short(v) if !v.is_empty() => Some(v[0] as u8),
            _ => None,
        });

    let data = ExifData {
        camera_make: get(Tag::Make),
        camera_model: get(Tag::Model),
        date_taken: get(Tag::DateTimeOriginal),
        gps_lat: gps_coord(Tag::GPSLatitude),
        gps_lon: gps_coord(Tag::GPSLongitude),
        orientation,
    };

    if data == ExifData::default() {
        Ok(None)
    } else {
        Ok(Some(data))
    }
}

/// EXIF orientation tag (values 1–8): how stored pixels relate to the
/// intended upright scene.
///
/// [`Orientation::apply`] converts stored pixels into upright pixels using
/// only lossless pixel shuffles (rotations and mirrors).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Orientation {
    /// Row 0 = visual top, column 0 = visual left. No transform.
    Up,
    /// Row 0 = top, column 0 = right: stored image is mirrored horizontally.
    MirrorHorizontal,
    /// Row 0 = bottom, column 0 = right: rotate 180°.
    Rotate180,
    /// Row 0 = bottom, column 0 = left: mirrored vertically.
    MirrorVertical,
    /// Row 0 = left, column 0 = top: transpose (flip across the main
    /// diagonal). Output dimensions are swapped.
    Transpose,
    /// Row 0 = right, column 0 = top: rotate 90° clockwise. Output
    /// dimensions are swapped.
    Rotate90Cw,
    /// Row 0 = right, column 0 = bottom: anti-transpose (flip across the
    /// anti-diagonal). Output dimensions are swapped.
    AntiTranspose,
    /// Row 0 = left, column 0 = bottom: rotate 90° counter-clockwise
    /// (270° clockwise). Output dimensions are swapped.
    Rotate270Cw,
}

impl Orientation {
    /// Map a raw EXIF orientation tag value (1–8). Returns [`None`] for
    /// anything else.
    #[must_use]
    pub fn from_u16(v: u16) -> Option<Self> {
        match v {
            1 => Some(Self::Up),
            2 => Some(Self::MirrorHorizontal),
            3 => Some(Self::Rotate180),
            4 => Some(Self::MirrorVertical),
            5 => Some(Self::Transpose),
            6 => Some(Self::Rotate90Cw),
            7 => Some(Self::AntiTranspose),
            8 => Some(Self::Rotate270Cw),
            _ => None,
        }
    }

    /// The raw EXIF tag value (1–8).
    #[must_use]
    pub fn to_u16(self) -> u16 {
        match self {
            Self::Up => 1,
            Self::MirrorHorizontal => 2,
            Self::Rotate180 => 3,
            Self::MirrorVertical => 4,
            Self::Transpose => 5,
            Self::Rotate90Cw => 6,
            Self::AntiTranspose => 7,
            Self::Rotate270Cw => 8,
        }
    }

    /// `true` when applying this orientation swaps width and height
    /// (orientations 5–8).
    #[must_use]
    pub fn swaps_dimensions(self) -> bool {
        matches!(
            self,
            Self::Transpose | Self::Rotate90Cw | Self::AntiTranspose | Self::Rotate270Cw
        )
    }

    /// Transform stored pixels into upright pixels (lossless pixel shuffle).
    #[must_use]
    pub fn apply(self, img: &DynamicImage) -> DynamicImage {
        match self {
            Self::Up => img.clone(),
            Self::MirrorHorizontal => img.fliph(),
            Self::Rotate180 => img.rotate180(),
            Self::MirrorVertical => img.flipv(),
            // transpose: verified against the EXIF spec placement rules by
            // `orientation_transforms_are_correct` in this module's tests.
            Self::Transpose => img.fliph().rotate270(),
            Self::Rotate90Cw => img.rotate90(),
            Self::AntiTranspose => img.flipv().rotate270(),
            Self::Rotate270Cw => img.rotate270(),
        }
    }
}

/// Read only the EXIF orientation tag from JPEG or TIFF bytes. Returns
/// [`None`] when the container has no EXIF or no orientation tag.
///
/// # Errors
///
/// [`MediaError::Exif`] when EXIF exists but cannot be parsed.
pub fn read_orientation(bytes: &[u8]) -> Result<Option<Orientation>, MediaError> {
    use exif::{In, Tag};

    let mut cursor = std::io::Cursor::new(bytes);
    let exif = match exif::Reader::new().read_from_container(&mut cursor) {
        Ok(e) => e,
        Err(exif::Error::NotFound(_)) => return Ok(None),
        Err(e) => return Err(MediaError::Exif(e.to_string())),
    };
    Ok(exif
        .get_field(Tag::Orientation, In::PRIMARY)
        .and_then(|f| match &f.value {
            exif::Value::Short(v) if !v.is_empty() => Orientation::from_u16(v[0]),
            _ => None,
        }))
}

/// Extract the raw EXIF block from JPEG/TIFF `source` bytes and wrap it as a
/// JPEG APP1 segment, suitable for re-injection into an encoded JPEG right
/// after the SOI marker.
///
/// This is the opt-in counterpart of the default metadata stripping: call it
/// *before* running the pipeline and splice the result (when `Some`) into
/// your JPEG output if you explicitly want to preserve camera metadata.
///
/// # Privacy
///
/// Preserved EXIF can contain GPS coordinates and device serial numbers.
/// Never attach this to third-party-visible output without review. Note that
/// if pixels were auto-oriented, the preserved orientation tag (0x0112) no
/// longer matches the transformed pixels.
///
/// # Errors
///
/// [`MediaError::Exif`] when EXIF exists but cannot be parsed.
pub fn app1_segment(source: &[u8]) -> Result<Option<Vec<u8>>, MediaError> {
    use exif::{In, Tag};

    let mut cursor = std::io::Cursor::new(source);
    let exif = match exif::Reader::new().read_from_container(&mut cursor) {
        Ok(e) => e,
        Err(exif::Error::NotFound(_)) => return Ok(None),
        Err(e) => return Err(MediaError::Exif(e.to_string())),
    };
    // A TIFF block with only zeroed padding carries nothing worth keeping.
    if exif.get_field(Tag::Orientation, In::PRIMARY).is_none()
        && exif.get_field(Tag::Make, In::PRIMARY).is_none()
        && exif.get_field(Tag::Model, In::PRIMARY).is_none()
        && exif.get_field(Tag::DateTimeOriginal, In::PRIMARY).is_none()
        && exif.get_field(Tag::GPSLatitude, In::PRIMARY).is_none()
    {
        return Ok(None);
    }
    let tiff = exif.buf();
    // APP1 segment: marker FFE1, u16 BE length (includes itself),
    // "Exif\0\0" signature, then the raw TIFF block.
    let payload_len = 6 + tiff.len();
    let mut out = Vec::with_capacity(4 + payload_len);
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(
        &u16::try_from(payload_len + 2)
            .map_err(|_| {
                MediaError::Exif("exif block too large for a JPEG APP1 segment".to_string())
            })?
            .to_be_bytes(),
    );
    out.extend_from_slice(b"Exif\0\0");
    out.extend_from_slice(tiff);
    Ok(Some(out))
}

/// Documentation string describing how stripping works in this crate.
///
/// The pipeline never copies metadata to its output: every encode step starts
/// from raw pixels, so EXIF/GPS data is dropped implicitly. `Op::StripExif`
/// exists purely as an explicit marker of that intent.
#[must_use]
pub fn strip_note() -> &'static str {
    "media-kit re-encodes from raw pixels, so all EXIF/GPS metadata is \
     stripped implicitly. Op::StripExif is an explicit no-op marker for \
     pipelines that must document metadata removal."
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;

    #[test]
    fn plain_jpeg_has_no_exif() {
        // The image crate's encoder writes no EXIF segment.
        assert_eq!(read_exif(&testutil::tiny_jpeg()).unwrap(), None);
    }

    #[test]
    fn png_has_no_exif() {
        assert_eq!(read_exif(&testutil::tiny_png(4, 4)).unwrap(), None);
    }

    #[test]
    fn garbage_is_error() {
        assert!(read_exif(b"garbage").is_err());
    }

    #[test]
    fn strip_note_mentions_implicit() {
        assert!(strip_note().contains("implicitly"));
    }

    #[test]
    fn orientation_from_u16_roundtrip() {
        for v in 1u16..=8 {
            let o = Orientation::from_u16(v).unwrap();
            assert_eq!(o.to_u16(), v);
        }
    }

    #[test]
    fn orientation_from_u16_rejects_out_of_range() {
        for v in [0u16, 9, 100, u16::MAX] {
            assert_eq!(Orientation::from_u16(v), None, "value {v}");
        }
    }

    #[test]
    fn orientation_swaps_dims_for_5_to_8() {
        for (v, swaps) in [
            (1, false),
            (2, false),
            (3, false),
            (4, false),
            (5, true),
            (6, true),
            (7, true),
            (8, true),
        ] {
            assert_eq!(Orientation::from_u16(v).unwrap().swaps_dimensions(), swaps);
        }
    }

    #[test]
    fn read_orientation_of_plain_jpeg_is_none() {
        assert_eq!(read_orientation(&testutil::tiny_jpeg()).unwrap(), None);
    }

    #[test]
    fn read_orientation_roundtrips_all_8() {
        for v in 1u16..=8 {
            let jpg = testutil::jpeg_with_orientation(v);
            assert_eq!(
                read_orientation(&jpg).unwrap(),
                Orientation::from_u16(v),
                "tag value {v}"
            );
        }
    }

    #[test]
    fn read_orientation_garbage_is_error() {
        assert!(read_orientation(b"junk").is_err());
    }

    #[test]
    fn read_exif_includes_orientation() {
        let jpg = testutil::jpeg_with_orientation(6);
        let data = read_exif(&jpg).unwrap().expect("orientation-only EXIF");
        assert_eq!(data.orientation, Some(6));
        assert_eq!(data.gps_lat, None);
    }

    /// Scene (2 wide, 3 tall); applying orientation `v` to the hand-written
    /// stored matrix for `v` must reproduce this exact scene. Stored
    /// matrices follow the EXIF spec placement rules ("0th row of stored
    /// image is the visual <left/right> side, 0th column is the visual
    /// <top/bottom>") and were derived by hand.
    #[test]
    fn orientation_transforms_are_correct() {
        use image::Luma;
        let scene = image::GrayImage::from_fn(2, 3, |x, y| Luma([(y * 2 + x + 1) as u8]));
        // stored matrices: k1–k4 are 2x3 (dims preserved), k5–k8 are 3x2.
        let stored: [(u16, &[[u8; 2]; 3]); 4] = [
            (1, &[[1, 2], [3, 4], [5, 6]]),
            (2, &[[2, 1], [4, 3], [6, 5]]),
            (3, &[[6, 5], [4, 3], [2, 1]]),
            (4, &[[5, 6], [3, 4], [1, 2]]),
        ];
        let stored_swapped: [(u16, &[[u8; 3]; 2]); 4] = [
            (5, &[[1, 3, 5], [2, 4, 6]]),
            (6, &[[2, 4, 6], [1, 3, 5]]),
            (7, &[[6, 4, 2], [5, 3, 1]]),
            (8, &[[5, 3, 1], [6, 4, 2]]),
        ];
        for (v, m) in stored {
            let img = image::GrayImage::from_fn(2, 3, |x, y| Luma([m[y as usize][x as usize]]));
            let out = Orientation::from_u16(v)
                .unwrap()
                .apply(&DynamicImage::ImageLuma8(img));
            assert_eq!(out.to_luma8().as_raw(), scene.as_raw(), "orientation {v}");
        }
        for (v, m) in stored_swapped {
            let img = image::GrayImage::from_fn(3, 2, |x, y| Luma([m[y as usize][x as usize]]));
            let out = Orientation::from_u16(v)
                .unwrap()
                .apply(&DynamicImage::ImageLuma8(img));
            assert_eq!(out.to_luma8().as_raw(), scene.as_raw(), "orientation {v}");
        }
    }

    #[test]
    fn apply_is_lossless_for_pixels() {
        // Applying an orientation then its inverse restores pixels exactly
        // (all transforms are lossless pixel shuffles).
        let img = testutil::noise_image(17, 9);
        let pairs = [
            (Orientation::Rotate90Cw, Orientation::Rotate270Cw),
            (Orientation::Transpose, Orientation::Transpose),
            (Orientation::MirrorHorizontal, Orientation::MirrorHorizontal),
            (Orientation::AntiTranspose, Orientation::AntiTranspose),
        ];
        for (a, b) in pairs {
            let out = b.apply(&a.apply(&img));
            assert_eq!(out.to_rgba8().as_raw(), img.to_rgba8().as_raw());
        }
    }

    #[test]
    fn app1_segment_none_for_plain_jpeg() {
        assert_eq!(app1_segment(&testutil::tiny_jpeg()).unwrap(), None);
    }

    #[test]
    fn app1_segment_roundtrip_preserves_orientation() {
        let src = testutil::jpeg_with_orientation(6);
        let app1 = app1_segment(&src).unwrap().expect("exif present");
        // Simulate the strip-then-preserve flow: pipeline output has no EXIF;
        // splicing the APP1 back restores it for the *opt-in* case.
        let stripped = testutil::tiny_jpeg();
        let mut out = Vec::new();
        out.extend_from_slice(&stripped[..2]);
        out.extend_from_slice(&app1);
        out.extend_from_slice(&stripped[2..]);
        assert_eq!(
            read_orientation(&out).unwrap(),
            Some(Orientation::Rotate90Cw)
        );
    }
}
