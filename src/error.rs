//! Error type for the whole pipeline.

use crate as _;

/// Every failure the pipeline can produce.
#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    /// Sniffing could not identify the input format.
    #[error("unsupported or unrecognized media format: {0}")]
    UnsupportedFormat(String),
    /// Decoding the image failed.
    #[error("decode error: {0}")]
    Decode(String),
    /// Encoding the output image failed.
    #[error("encode error: {0}")]
    Encode(String),
    /// Input exceeded the configured byte-size limit (decompression-bomb guard).
    #[error("input too large: got {got} bytes, limit is {limit_bytes}")]
    TooLarge {
        /// Configured maximum input size in bytes.
        limit_bytes: u64,
        /// Actual input size in bytes.
        got: u64,
    },
    /// Decoded dimensions exceeded the configured limits.
    #[error("image dimensions too large: got {got}, limit is {max}")]
    DimensionsTooLarge {
        /// Configured maximum (width or height, whichever applies).
        max: u32,
        /// Actual offending dimension.
        got: u32,
    },
    /// Underlying I/O failure.
    #[cfg(feature = "std")]
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// EXIF parsing failed.
    #[cfg(feature = "exif")]
    #[error("exif error: {0}")]
    Exif(String),
}
