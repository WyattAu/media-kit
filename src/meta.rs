//! Header-only metadata inspection and decompression-bomb guards.

use crate::error::MediaError;

/// Resource limits applied before decoding (decompression-bomb guard).
///
/// Defaults: 10 MiB input, 8192x8192 max dimensions.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Limits {
    /// Maximum accepted input size in bytes. Default: `10 * 1024 * 1024`.
    pub max_bytes: u64,
    /// Maximum accepted image width. Default: `8192`.
    pub max_width: u32,
    /// Maximum accepted image height. Default: `8192`.
    pub max_height: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: 10 * 1024 * 1024,
            max_width: 8192,
            max_height: 8192,
        }
    }
}

impl Limits {
    /// Defaults.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the byte-size limit.
    #[must_use]
    pub fn max_bytes(mut self, v: u64) -> Self {
        self.max_bytes = v;
        self
    }

    /// Set the width limit.
    #[must_use]
    pub fn max_width(mut self, v: u32) -> Self {
        self.max_width = v;
        self
    }

    /// Set the height limit.
    #[must_use]
    pub fn max_height(mut self, v: u32) -> Self {
        self.max_height = v;
        self
    }
}

/// Image dimensions `(width, height)` parsed header-only, without decoding
/// pixel data. Returns [`None`] when the format is unknown or the header is
/// unreadable.
#[must_use]
pub fn dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let cursor = std::io::Cursor::new(bytes);
    image::ImageReader::new(cursor)
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

/// Check byte size and decoded dimensions against `limits`.
///
/// Dimensions are read header-only (no pixel decode) so hostile headers are
/// rejected before any allocation happens.
///
/// # Errors
///
/// - [`MediaError::TooLarge`] when input exceeds `max_bytes`
/// - [`MediaError::UnsupportedFormat`] when the format cannot be sniffed
/// - [`MediaError::DimensionsTooLarge`] when width or height exceed limits
/// - [`MediaError::Decode`] when the header cannot be read at all
pub fn enforce_limits(bytes: &[u8], limits: &Limits) -> Result<(), MediaError> {
    let got = bytes.len() as u64;
    if got > limits.max_bytes {
        return Err(MediaError::TooLarge {
            limit_bytes: limits.max_bytes,
            got,
        });
    }
    let Some((w, h)) = dimensions(bytes) else {
        return Err(MediaError::UnsupportedFormat(
            "could not read image header".into(),
        ));
    };
    if w > limits.max_width {
        return Err(MediaError::DimensionsTooLarge {
            max: limits.max_width,
            got: w,
        });
    }
    if h > limits.max_height {
        return Err(MediaError::DimensionsTooLarge {
            max: limits.max_height,
            got: h,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(any(feature = "jpeg", feature = "png"))]
    use crate::testutil;

    #[test]
    fn defaults() {
        let l = Limits::default();
        assert_eq!(l.max_bytes, 10 * 1024 * 1024);
        assert_eq!(l.max_width, 8192);
        assert_eq!(l.max_height, 8192);
    }

    #[test]
    fn builder() {
        let l = Limits::new().max_bytes(1024).max_width(100).max_height(200);
        assert_eq!((l.max_bytes, l.max_width, l.max_height), (1024, 100, 200));
    }

    #[test]
    #[cfg(feature = "png")]
    fn dimensions_of_png() {
        let png = testutil::tiny_png(64, 32);
        assert_eq!(dimensions(&png), Some((64, 32)));
    }

    #[test]
    #[cfg(feature = "jpeg")]
    fn dimensions_of_jpeg() {
        let jpg = testutil::tiny_jpeg();
        assert_eq!(dimensions(&jpg), Some((8, 8)));
    }

    #[test]
    fn dimensions_garbage_is_none() {
        assert_eq!(dimensions(b"garbage"), None);
        assert_eq!(dimensions(&[]), None);
    }

    #[test]
    #[cfg(feature = "jpeg")]
    fn accept_within_limits() {
        let jpg = testutil::tiny_jpeg();
        assert!(enforce_limits(&jpg, &Limits::default()).is_ok());
    }

    #[test]
    #[cfg(feature = "jpeg")]
    fn reject_oversize_bytes() {
        let jpg = testutil::tiny_jpeg();
        let err = enforce_limits(&jpg, &Limits::new().max_bytes(4)).unwrap_err();
        match err {
            MediaError::TooLarge { got, .. } => assert_eq!(got, jpg.len() as u64),
            other => panic!("wrong error: {other}"),
        }
    }

    #[test]
    #[cfg(feature = "png")]
    fn reject_oversize_dims() {
        let png = testutil::tiny_png(64, 32);
        let err = enforce_limits(&png, &Limits::new().max_width(16)).unwrap_err();
        assert!(matches!(
            err,
            MediaError::DimensionsTooLarge { max: 16, got: 64 }
        ));
        let err = enforce_limits(&png, &Limits::new().max_height(8)).unwrap_err();
        assert!(matches!(
            err,
            MediaError::DimensionsTooLarge { max: 8, got: 32 }
        ));
    }

    #[test]
    fn reject_unknown_header() {
        let err = enforce_limits(b"garbage", &Limits::default()).unwrap_err();
        assert!(matches!(err, MediaError::UnsupportedFormat(_)));
    }
}
