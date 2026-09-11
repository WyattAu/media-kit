//! High-level decode -> transform -> encode pipeline with bomb-guarding.

use std::vec::Vec;

use image::DynamicImage;

use crate::encode::OutFormat;
use crate::error::MediaError;
use crate::meta::{enforce_limits, Limits};
use crate::resize::{Filter, Fit};
use crate::sniff;

/// A single operation in the pipeline.
#[derive(Clone)]
pub enum Op {
    /// Resize to the given fit with the given filter.
    Resize(Fit, Filter),
    /// Paste an overlay image at `(x, y)` with the given opacity.
    Overlay {
        /// Overlay source image.
        img: DynamicImage,
        /// Top-left paste position (may be negative).
        x: i64,
        /// Top-left paste position (may be negative).
        y: i64,
        /// Overlay opacity in `[0.0, 1.0]`.
        opacity: f32,
    },
    /// Explicit intent marker: re-encoding from raw pixels already drops all
    /// metadata (EXIF/GPS/ICC), so this is a no-op kept for auditability.
    StripExif,
}

impl std::fmt::Debug for Op {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Op::Resize(fit, filter) => write!(f, "Resize({fit:?}, {filter:?})"),
            Op::Overlay { x, y, opacity, .. } => {
                write!(f, "Overlay {{ x: {x}, y: {y}, opacity: {opacity} }}")
            }
            Op::StripExif => f.write_str("StripExif"),
        }
    }
}

/// Configured decode -> transform -> encode pipeline.
///
/// # Example
///
/// ```
/// # fn main() -> Result<(), media_kit::MediaError> {
/// # #[cfg(feature = "jpeg")]
/// # {
/// use media_kit::pipeline::Pipeline;
/// use media_kit::resize::{Fit, Filter};
/// use media_kit::encode::OutFormat;
///
/// let bytes = media_kit::testutil::tiny_jpeg();
/// let webp = Pipeline::new(OutFormat::WebP(None))
///     .resize(Fit::MaxSide(4), Filter::Lanczos3)
///     .run(&bytes)?;
/// assert!(!webp.is_empty());
/// # }
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct Pipeline {
    limits: Limits,
    ops: Vec<Op>,
    out: OutFormat,
}

impl Pipeline {
    /// Pipeline with default [`Limits`] and no ops.
    #[must_use]
    pub fn new(out: OutFormat) -> Self {
        Self {
            limits: Limits::default(),
            ops: Vec::new(),
            out,
        }
    }

    /// Replace the resource limits.
    #[must_use]
    pub fn limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// Append a resize op.
    #[must_use]
    pub fn resize(mut self, fit: Fit, filter: Filter) -> Self {
        self.ops.push(Op::Resize(fit, filter));
        self
    }

    /// Append an overlay op.
    #[must_use]
    pub fn overlay(mut self, img: DynamicImage, x: i64, y: i64, opacity: f32) -> Self {
        self.ops.push(Op::Overlay { img, x, y, opacity });
        self
    }

    /// Append the explicit metadata-strip marker (no-op; re-encode strips).
    #[must_use]
    pub fn strip_exif(mut self) -> Self {
        self.ops.push(Op::StripExif);
        self
    }

    /// Run the full pipeline on raw encoded `bytes`:
    ///
    /// 1. sniff format
    /// 2. enforce limits (byte size + header dimensions)
    /// 3. decode
    /// 4. apply ops in order
    /// 5. encode to the configured output format
    ///
    /// # Errors
    ///
    /// [`MediaError`] for unknown format, limit violations, decode or encode
    /// failures.
    pub fn run(&self, bytes: &[u8]) -> Result<Vec<u8>, MediaError> {
        if sniff::sniff(bytes).is_none() {
            return Err(MediaError::UnsupportedFormat(
                "input is not a recognized media format".into(),
            ));
        }
        enforce_limits(bytes, &self.limits)?;
        let mut img =
            image::load_from_memory(bytes).map_err(|e| MediaError::Decode(e.to_string()))?;
        for op in &self.ops {
            img = apply_op(&img, op)?;
        }
        crate::encode::encode(&img, &self.out)
    }

    /// Async variant of [`Pipeline::run`]: blocking work is moved off the
    /// async runtime via [`tokio::task::spawn_blocking`]. Requires the
    /// `async` feature.
    ///
    /// # Errors
    ///
    /// Same as [`Pipeline::run`]; additionally propagates join errors as
    /// [`MediaError::Io`].
    #[cfg(feature = "async")]
    pub async fn run_async(&self, bytes: Vec<u8>) -> Result<Vec<u8>, MediaError> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || this.run(&bytes))
            .await
            .map_err(|e| MediaError::Io(std::io::Error::other(e.to_string())))?
    }
}

fn apply_op(img: &DynamicImage, op: &Op) -> Result<DynamicImage, MediaError> {
    Ok(match op {
        Op::Resize(fit, filter) => crate::resize::resize(img, fit, *filter),
        Op::Overlay {
            img: over,
            x,
            y,
            opacity,
        } => crate::composite::overlay(
            img,
            over,
            *x,
            *y,
            crate::composite::Blend::Over { opacity: *opacity },
        ),
        Op::StripExif => img.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(any(feature = "jpeg", feature = "png"))]
    use crate::testutil;

    #[test]
    #[cfg(feature = "jpeg")]
    fn thumb_to_webp_end_to_end() {
        let jpg = testutil::tiny_jpeg();
        let out = Pipeline::new(OutFormat::WebP(None))
            .resize(Fit::MaxSide(4), Filter::Lanczos3)
            .strip_exif()
            .run(&jpg)
            .unwrap();
        assert_eq!(sniff::sniff(&out), Some(sniff::Format::WebP));
        let img = image::load_from_memory(&out).unwrap();
        assert_eq!((img.width(), img.height()), (4, 4));
    }

    #[test]
    #[cfg(feature = "png")]
    fn jpeg_output_with_quality() {
        let png = testutil::tiny_png(16, 16);
        let out = Pipeline::new(OutFormat::Jpeg(90))
            .resize(Fit::MaxSide(8), Filter::CatmullRom)
            .run(&png)
            .unwrap();
        assert_eq!(sniff::sniff(&out), Some(sniff::Format::Jpeg));
    }

    #[test]
    fn rejects_garbage() {
        let err = Pipeline::new(OutFormat::Png).run(b"garbage").unwrap_err();
        assert!(matches!(err, MediaError::UnsupportedFormat(_)));
    }

    #[test]
    #[cfg(feature = "jpeg")]
    fn rejects_oversize() {
        let jpg = testutil::tiny_jpeg();
        let err = Pipeline::new(OutFormat::Png)
            .limits(Limits::new().max_bytes(4))
            .run(&jpg)
            .unwrap_err();
        assert!(matches!(err, MediaError::TooLarge { .. }));
    }

    #[test]
    #[cfg(feature = "jpeg")]
    fn overlay_op_runs() {
        let jpg = testutil::tiny_jpeg();
        let over = DynamicImage::new_rgba8(2, 2);
        let out = Pipeline::new(OutFormat::Png)
            .overlay(over, 0, 0, 0.5)
            .run(&jpg)
            .unwrap();
        assert_eq!(sniff::sniff(&out), Some(sniff::Format::Png));
    }

    #[test]
    #[cfg(feature = "jpeg")]
    fn debug_hides_overlay_pixels() {
        let p = Pipeline::new(OutFormat::Png)
            .overlay(DynamicImage::new_rgba8(1, 1), 0, 0, 0.5)
            .strip_exif();
        let dbg = format!("{p:?}");
        assert!(dbg.contains("Overlay"));
        assert!(dbg.contains("StripExif"));
    }

    #[cfg(feature = "async")]
    #[tokio::test]
    async fn async_run_works() {
        let jpg = testutil::tiny_jpeg();
        let out = Pipeline::new(OutFormat::WebP(None))
            .resize(Fit::MaxSide(4), Filter::Triangle)
            .run_async(jpg)
            .await
            .unwrap();
        assert_eq!(sniff::sniff(&out), Some(sniff::Format::WebP));
    }

    #[cfg(feature = "async")]
    #[tokio::test]
    async fn async_rejects_garbage() {
        let err = Pipeline::new(OutFormat::Png)
            .run_async(b"nope".to_vec())
            .await
            .unwrap_err();
        assert!(matches!(err, MediaError::UnsupportedFormat(_)));
    }
}
