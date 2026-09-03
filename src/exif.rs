//! EXIF metadata extraction (behind the `exif` feature).
//!
//! Note: the pipeline re-encodes images, which drops all metadata. This is
//! intentional — see [`strip_note`].

use crate::error::MediaError;

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

    let data = ExifData {
        camera_make: get(Tag::Make),
        camera_model: get(Tag::Model),
        date_taken: get(Tag::DateTimeOriginal),
        gps_lat: gps_coord(Tag::GPSLatitude),
        gps_lon: gps_coord(Tag::GPSLongitude),
    };

    if data == ExifData::default() {
        Ok(None)
    } else {
        Ok(Some(data))
    }
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
}
