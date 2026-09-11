//! Full image pipeline — sniff, meta, resize, encode, composite, variants
//! with bomb-guard and async support.
//!
//! This crate is std-only (the [`image`] decoder stack requires std).
//!
//! # Example
//!
//! ```
//! # fn main() -> Result<(), media_kit::MediaError> {
//! # #[cfg(feature = "jpeg")]
//! # {
//! use media_kit::pipeline::Pipeline;
//! use media_kit::resize::{Fit, Filter};
//! use media_kit::encode::OutFormat;
//!
//! let jpeg = media_kit::testutil::tiny_jpeg();
//! let out = Pipeline::new(OutFormat::WebP(None))
//!     .resize(Fit::MaxSide(200), Filter::Lanczos3)
//!     .run(&jpeg)?;
//! assert!(!out.is_empty());
//! # }
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))] // tests assert invariants directly
#![deny(missing_docs)]

pub mod composite;
pub mod encode;
pub mod error;
pub mod meta;
pub mod pipeline;
pub mod resize;
pub mod sniff;
pub mod variants;

#[cfg(feature = "exif")]
pub mod exif;

/// Sample media used by doctests, integration tests, and benches.
pub mod testutil;

pub use error::MediaError;
