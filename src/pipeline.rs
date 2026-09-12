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
    // Only consumed under the `exif` feature (see `run`).
    #[cfg_attr(not(feature = "exif"), allow(dead_code))]
    auto_orient: bool,
}

impl Pipeline {
    /// Pipeline with default [`Limits`] and no ops.
    #[must_use]
    pub fn new(out: OutFormat) -> Self {
        Self {
            limits: Limits::default(),
            ops: Vec::new(),
            out,
            auto_orient: false,
        }
    }

    /// Replace the resource limits.
    #[must_use]
    pub fn limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// Enable EXIF auto-orientation (requires the `exif` feature, on by
    /// default): when the input carries an EXIF orientation tag, pixels are
    /// rotated/mirrored upright right after decode, before any op. The
    /// output is a re-encode, so the tag itself is stripped — output pixels
    /// are upright and tag-free.
    #[cfg(feature = "exif")]
    #[must_use]
    pub fn auto_orient(mut self, on: bool) -> Self {
        self.auto_orient = on;
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
    /// 4. auto-orient from EXIF (when enabled, [`Self::auto_orient`])
    /// 5. apply ops in order
    /// 6. encode to the configured output format
    ///
    /// # Privacy
    ///
    /// The output is re-encoded from raw pixels: no EXIF/GPS metadata is
    /// ever carried over. See [`crate::exif::app1_segment`] for opt-in
    /// preservation.
    ///
    /// # Errors
    ///
    /// [`MediaError`] for unknown format, limit violations, decode, EXIF or
    /// encode failures.
    pub fn run(&self, bytes: &[u8]) -> Result<Vec<u8>, MediaError> {
        if sniff::sniff(bytes).is_none() {
            return Err(MediaError::UnsupportedFormat(
                "input is not a recognized media format".into(),
            ));
        }
        enforce_limits(bytes, &self.limits)?;
        let mut img =
            image::load_from_memory(bytes).map_err(|e| MediaError::Decode(e.to_string()))?;
        #[cfg(feature = "exif")]
        if self.auto_orient {
            if let Some(o) = crate::exif::read_orientation(bytes)? {
                img = o.apply(&img);
            }
        }
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

    #[cfg(all(feature = "exif", feature = "jpeg"))]
    #[test]
    fn auto_orient_uprights_rotated_input() {
        use crate::testutil;
        use image::GenericImageView;
        use image::Luma;

        // Non-square sideways scene: 3 wide × 2 tall upright, stored 2×3
        // with EXIF orientation 6 (view by rotating 90° CW). We splice the
        // crafted APP1 onto a JPEG whose pixels are the sideways scene.
        let scene = image::GrayImage::from_fn(3, 2, |x, y| Luma([(y * 3 + x + 1) as u8]));
        let sideways = image::DynamicImage::ImageLuma8(scene).rotate270(); // inverse of CW
        let jpg_bytes = crate::encode::encode(&sideways, &OutFormat::Jpeg(90)).unwrap();
        let app1 = crate::exif::app1_segment(&testutil::jpeg_with_orientation(6))
            .unwrap()
            .unwrap();
        let mut tagged = Vec::new();
        tagged.extend_from_slice(&jpg_bytes[..2]);
        tagged.extend_from_slice(&app1);
        tagged.extend_from_slice(&jpg_bytes[2..]);

        // With auto-orient the pixels come out upright (matching the scene
        // modulo JPEG loss), 2 tall × 3 wide.
        let out = Pipeline::new(OutFormat::Png)
            .auto_orient(true)
            .run(&tagged)
            .unwrap();
        let img = image::load_from_memory(&out).unwrap();
        assert_eq!((img.width(), img.height()), (3, 2));
        for y in 0..2u32 {
            for x in 0..3u32 {
                let want = (y * 3 + x + 1) as u8;
                let got = img.get_pixel(x, y).0[0];
                assert!(
                    (i16::from(got) - i16::from(want)).abs() <= 8,
                    "({x},{y}): got {got}, want {want}"
                );
            }
        }

        // Without auto-orient the stored (sideways) pixels pass through.
        let out_off = Pipeline::new(OutFormat::Png).run(&tagged).unwrap();
        let img_off = image::load_from_memory(&out_off).unwrap();
        assert_eq!((img_off.width(), img_off.height()), (2, 3));
    }

    #[cfg(all(feature = "exif", feature = "png"))]
    #[test]
    fn auto_orient_non_square_swap() {
        use crate::testutil;

        // Craft a non-square EXIF JPEG: splice the orientation-8 APP1 onto a
        // 16x8 PNG-decoded JPEG encode so the swap is visible in dims.
        let wide = testutil::noise_image(16, 8);
        let jpg_bytes = crate::encode::encode(&wide, &OutFormat::Jpeg(90)).unwrap();
        let app1 = crate::exif::app1_segment(&testutil::jpeg_with_orientation(8))
            .unwrap()
            .unwrap();
        let mut tagged = Vec::new();
        tagged.extend_from_slice(&jpg_bytes[..2]);
        tagged.extend_from_slice(&app1);
        tagged.extend_from_slice(&jpg_bytes[2..]);
        assert_eq!(
            crate::exif::read_orientation(&tagged).unwrap(),
            Some(crate::exif::Orientation::Rotate270Cw)
        );

        let out = Pipeline::new(OutFormat::Png)
            .auto_orient(true)
            .run(&tagged)
            .unwrap();
        let img = image::load_from_memory(&out).unwrap();
        assert_eq!((img.width(), img.height()), (8, 16), "dims must swap");
        assert_eq!(crate::exif::read_exif(&out).unwrap(), None, "stripped");

        // Without auto-orient the dims are untouched.
        let out_off = Pipeline::new(OutFormat::Png).run(&tagged).unwrap();
        let img_off = image::load_from_memory(&out_off).unwrap();
        assert_eq!((img_off.width(), img_off.height()), (16, 8));
    }

    #[cfg(feature = "exif")]
    #[test]
    fn auto_orient_without_exif_is_noop() {
        use crate::testutil;
        let jpg = testutil::tiny_jpeg();
        let a = Pipeline::new(OutFormat::Png)
            .auto_orient(true)
            .run(&jpg)
            .unwrap();
        let b = Pipeline::new(OutFormat::Png).run(&jpg).unwrap();
        assert_eq!(a, b);
    }

    #[cfg(all(feature = "exif", feature = "jpeg"))]
    #[test]
    fn preserve_exif_opt_in_after_strip() {
        use crate::testutil;

        let src = testutil::jpeg_with_orientation(3);
        let out = Pipeline::new(OutFormat::Jpeg(90))
            .auto_orient(true)
            .run(&src)
            .unwrap();
        // Default privacy: nothing survives a re-encode.
        assert_eq!(crate::exif::read_exif(&out).unwrap(), None);

        // Opt-in: splice the original APP1 back.
        let app1 = crate::exif::app1_segment(&src).unwrap().unwrap();
        let mut preserved = Vec::new();
        preserved.extend_from_slice(&out[..2]);
        preserved.extend_from_slice(&app1);
        preserved.extend_from_slice(&out[2..]);
        let data = crate::exif::read_exif(&preserved).unwrap().unwrap();
        assert_eq!(data.orientation, Some(3));
    }
}
