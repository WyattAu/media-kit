//! Multi-size output generation ("thumb", "medium", "large", ...).
//!
//! # Parallel fan-out
//!
//! With the `parallel` feature (on by default), [`VariantSet::generate`]
//! fans the per-variant resize+encode work out across the [`rayon`] thread
//! pool: one decoded image, N variants in parallel. Use
//! [`VariantSet::generate_serial`] when you need guaranteed single-thread
//! execution (embedded, deterministic timing, already inside a rayon pool).
//!
//! Decoded input and per-variant outputs are independent, so results are
//! byte-identical between serial and parallel runs for deterministic codecs
//! (PNG, JPEG, WebP lossless).
//!
//! # Decode once, generate many
//!
//! [`VariantSet::generate_from_bytes`] accepts raw encoded bytes: it sniffs,
//! bomb-guards, decodes once, auto-orients from EXIF (with the `exif`
//! feature) and fans out — the recommended entry point for user uploads.

use std::vec::Vec;

use crate::encode::OutFormat;
use crate::error::MediaError;
use crate::meta::Limits;
use crate::resize::{resize, Filter, Fit};

/// Named output configuration.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Variant {
    /// Human/stable name, e.g. `"thumb"`. Becomes the output key.
    pub name: String,
    /// Target geometry.
    pub fit: Fit,
    /// Target encoding.
    pub format: OutFormat,
    /// Resampling filter. Defaults to [`Filter::CatmullRom`].
    pub filter: Filter,
}

impl Variant {
    /// New variant with the default filter.
    #[must_use]
    pub fn new(name: impl Into<String>, fit: Fit, format: OutFormat) -> Self {
        Self {
            name: name.into(),
            fit,
            format,
            filter: Filter::CatmullRom,
        }
    }

    /// Override the resampling filter.
    #[must_use]
    pub fn filter(mut self, filter: Filter) -> Self {
        self.filter = filter;
        self
    }
}

/// A configured collection of variants.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VariantSet {
    /// Ordered variants.
    pub variants: Vec<Variant>,
}

/// One generated variant: `(name, format, encoded bytes)`, in declaration
/// order.
pub type VariantOutput = (String, OutFormat, Vec<u8>);

fn encode_variant(
    img: &image::DynamicImage,
    variant: &Variant,
) -> Result<VariantOutput, MediaError> {
    let resized = resize(img, &variant.fit, variant.filter);
    let bytes = crate::encode::encode(&resized, &variant.format)?;
    Ok((variant.name.clone(), variant.format, bytes))
}

impl VariantSet {
    /// Empty set. Add with [`push`](Self::push) or [`with`](Self::with).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a variant (builder style).
    #[must_use]
    pub fn with(mut self, variant: Variant) -> Self {
        self.variants.push(variant);
        self
    }

    /// Add a variant (mutating style).
    pub fn push(&mut self, variant: Variant) {
        self.variants.push(variant);
    }

    /// Common web preset:
    /// - `thumb`  — MaxSide 200, WebP lossless
    /// - `medium` — Width 800, WebP lossless
    /// - `large`  — Width 1920, WebP lossless
    #[must_use]
    pub fn web_standard() -> Self {
        Self::new()
            .with(Variant::new(
                "thumb",
                Fit::MaxSide(200),
                OutFormat::WebP(None),
            ))
            .with(Variant::new(
                "medium",
                Fit::Width(800),
                OutFormat::WebP(None),
            ))
            .with(Variant::new(
                "large",
                Fit::Width(1920),
                OutFormat::WebP(None),
            ))
    }

    /// Encode one decoded image into every variant, in declaration order.
    ///
    /// With the `parallel` feature this fans the resize+encode work out
    /// across the [`rayon`] thread pool (decode once, generate in parallel);
    /// otherwise it runs serially. Output bytes are identical either way for
    /// deterministic codecs.
    ///
    /// # Errors
    ///
    /// [`MediaError::Encode`] when any variant fails to encode.
    #[cfg(feature = "parallel")]
    pub fn generate(&self, img: &image::DynamicImage) -> Result<Vec<VariantOutput>, MediaError> {
        use rayon::prelude::*;

        self.variants
            .par_iter()
            .map(|v| encode_variant(img, v))
            .collect()
    }

    /// Serial counterpart of [`Self::generate`] — always single-threaded,
    /// regardless of features.
    ///
    /// # Errors
    ///
    /// [`MediaError::Encode`] when any variant fails to encode.
    pub fn generate_serial(
        &self,
        img: &image::DynamicImage,
    ) -> Result<Vec<VariantOutput>, MediaError> {
        self.variants
            .iter()
            .map(|v| encode_variant(img, v))
            .collect()
    }

    /// Convenience: encode one decoded image into every variant without
    /// parallelism even when the `parallel` feature is enabled. Alias of
    /// [`Self::generate_serial`].
    ///
    /// # Errors
    ///
    /// [`MediaError::Encode`] when any variant fails to encode.
    #[cfg(not(feature = "parallel"))]
    pub fn generate(&self, img: &image::DynamicImage) -> Result<Vec<VariantOutput>, MediaError> {
        self.generate_serial(img)
    }

    /// Decode raw encoded `bytes` once (sniff → bomb-guard → decode → EXIF
    /// auto-orient) and generate every variant from the single decode.
    ///
    /// This is the recommended entry point for uploads: the input is
    /// decoded exactly once regardless of variant count, and pixels are
    /// rotated upright when the source carries an EXIF orientation tag (with
    /// the `exif` feature), so all variants come out upright.
    ///
    /// # Privacy
    ///
    /// Variants are re-encoded from raw pixels: no EXIF/GPS metadata is
    /// carried into any output. See [`crate::exif`] for extraction and
    /// opt-in preservation.
    ///
    /// # Errors
    ///
    /// [`MediaError`] for unknown format, limit violations, decode, EXIF or
    /// encode failures.
    pub fn generate_from_bytes(&self, bytes: &[u8]) -> Result<Vec<VariantOutput>, MediaError> {
        self.generate_from_bytes_with_limits(bytes, &Limits::default())
    }

    /// [`Self::generate_from_bytes`] with explicit [`Limits`].
    ///
    /// # Errors
    ///
    /// As [`Self::generate_from_bytes`].
    pub fn generate_from_bytes_with_limits(
        &self,
        bytes: &[u8],
        limits: &Limits,
    ) -> Result<Vec<VariantOutput>, MediaError> {
        crate::meta::enforce_limits(bytes, limits)?;
        let img = image::load_from_memory(bytes).map_err(|e| MediaError::Decode(e.to_string()))?;
        #[cfg(feature = "exif")]
        let img = match crate::exif::read_orientation(bytes)? {
            Some(o) => o.apply(&img),
            None => img,
        };
        self.generate(&img)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    fn img(w: u32, h: u32) -> image::DynamicImage {
        image::DynamicImage::ImageRgba8(RgbaImage::from_pixel(w, h, image::Rgba([7, 8, 9, 255])))
    }

    #[test]
    fn web_standard_shape() {
        let set = VariantSet::web_standard();
        assert_eq!(set.variants.len(), 3);
        assert_eq!(set.variants[0].name, "thumb");
        assert_eq!(set.variants[1].name, "medium");
        assert_eq!(set.variants[2].name, "large");
        assert_eq!(set.variants[0].fit, Fit::MaxSide(200));
        assert_eq!(set.variants[1].fit, Fit::Width(800));
        assert_eq!(set.variants[2].fit, Fit::Width(1920));
    }

    #[test]
    #[cfg(feature = "webp")]
    fn web_standard_produces_three_outputs_with_expected_dims() {
        let set = VariantSet::web_standard();
        let out = set.generate(&img(2048, 1536)).unwrap();
        assert_eq!(out.len(), 3);
        let (_, fmt, bytes) = &out[0];
        assert_eq!(*fmt, OutFormat::WebP(None));
        let thumb = image::load_from_memory(bytes).unwrap();
        assert_eq!((thumb.width(), thumb.height()), (200, 150));
        let medium = image::load_from_memory(&out[1].2).unwrap();
        assert_eq!((medium.width(), medium.height()), (800, 600));
        let large = image::load_from_memory(&out[2].2).unwrap();
        assert_eq!((large.width(), large.height()), (1920, 1440));
    }

    #[test]
    #[cfg(feature = "webp")]
    fn smaller_source_is_not_upscaled() {
        let set = VariantSet::web_standard();
        let out = set.generate(&img(600, 400)).unwrap();
        let large = image::load_from_memory(&out[2].2).unwrap();
        assert_eq!((large.width(), large.height()), (600, 400));
    }

    #[test]
    #[cfg(feature = "jpeg")]
    fn custom_set_mixed_formats() {
        let set = VariantSet::new()
            .with(Variant::new(
                "avatar",
                Fit::Cover(64, 64),
                OutFormat::Jpeg(85),
            ))
            .with(Variant::new("orig", Fit::MaxSide(50), OutFormat::Png));
        let out = set.generate(&img(300, 100)).unwrap();
        assert_eq!(out.len(), 2);
        let avatar = image::load_from_memory(&out[0].2).unwrap();
        assert_eq!((avatar.width(), avatar.height()), (64, 64));
        assert_eq!(out[0].1, OutFormat::Jpeg(85));
        assert_eq!(out[1].1, OutFormat::Png);
    }

    #[test]
    fn empty_set_yields_empty() {
        assert!(VariantSet::new().generate(&img(10, 10)).unwrap().is_empty());
    }

    #[test]
    fn variant_filter_builder() {
        let v = Variant::new("x", Fit::Width(10), OutFormat::Png).filter(Filter::Lanczos3);
        assert_eq!(v.filter, Filter::Lanczos3);
    }

    #[test]
    #[cfg(all(feature = "png", feature = "jpeg"))]
    fn serial_and_parallel_outputs_are_byte_identical() {
        use crate::testutil;

        // Deterministic codecs: parallel fan-out must not change a byte.
        let set = VariantSet::new()
            .with(Variant::new("a", Fit::Width(64), OutFormat::Png))
            .with(Variant::new("b", Fit::Width(48), OutFormat::Jpeg(80)))
            .with(Variant::new("c", Fit::MaxSide(32), OutFormat::Png));
        let img = testutil::noise_image(300, 200);
        let serial = set.generate_serial(&img).unwrap();
        let parallel = set.generate(&img).unwrap();
        assert_eq!(serial.len(), 3);
        assert_eq!(parallel.len(), 3);
        for (s, p) in serial.iter().zip(parallel.iter()) {
            assert_eq!(s, p, "variant {:?} diverged", s.0);
        }
    }

    #[test]
    #[cfg(feature = "webp")]
    fn serial_and_parallel_webp_lossless_byte_identical() {
        use crate::testutil;

        let set = VariantSet::new()
            .with(Variant::new("w", Fit::Width(40), OutFormat::WebP(None)))
            .with(Variant::new("z", Fit::Width(20), OutFormat::WebP(None)));
        let img = testutil::noise_image(120, 90);
        assert_eq!(
            set.generate_serial(&img).unwrap(),
            set.generate(&img).unwrap()
        );
    }

    #[test]
    #[cfg(feature = "jpeg")]
    fn quality_parameter_lands_in_variant_output() {
        use crate::testutil;

        let noisy = testutil::noise_image(256, 256);
        let set = VariantSet::new()
            .with(Variant::new("hi", Fit::Width(200), OutFormat::Jpeg(95)))
            .with(Variant::new("lo", Fit::Width(200), OutFormat::Jpeg(15)));
        let out = set.generate(&noisy).unwrap();
        assert_eq!(out[0].0, "hi");
        assert_eq!(out[1].0, "lo");
        assert!(
            out[0].2.len() > out[1].2.len(),
            "q95 ({} B) must exceed q15 ({} B)",
            out[0].2.len(),
            out[1].2.len()
        );
    }

    #[test]
    #[cfg(all(feature = "webp", feature = "webp-lossy", feature = "jpeg"))]
    fn webp_lossy_quality_lands_in_variant_output() {
        use crate::testutil;

        let noisy = testutil::noise_image(256, 256);
        let set = VariantSet::new()
            .with(Variant::new(
                "hi",
                Fit::Width(200),
                OutFormat::WebP(Some(90.0)),
            ))
            .with(Variant::new(
                "lo",
                Fit::Width(200),
                OutFormat::WebP(Some(20.0)),
            ));
        let out = set.generate(&noisy).unwrap();
        assert!(
            out[0].2.len() > out[1].2.len(),
            "webp q90 ({} B) must exceed q20 ({} B)",
            out[0].2.len(),
            out[1].2.len()
        );
    }

    #[test]
    #[cfg(all(feature = "webp", feature = "jpeg"))]
    fn generate_from_bytes_decodes_once_and_fans_out() {
        use crate::testutil;

        let set = VariantSet::new()
            .with(Variant::new(
                "thumb",
                Fit::MaxSide(16),
                OutFormat::WebP(None),
            ))
            .with(Variant::new("mid", Fit::Width(24), OutFormat::Jpeg(85)));
        let src = testutil::noise_image(64, 48);
        let jpg = crate::encode::encode(&src, &OutFormat::Jpeg(90)).unwrap();
        let out = set.generate_from_bytes(&jpg).unwrap();
        assert_eq!(out.len(), 2);
        let thumb = image::load_from_memory(&out[0].2).unwrap();
        assert_eq!((thumb.width(), thumb.height()), (16, 12));
        let mid = image::load_from_memory(&out[1].2).unwrap();
        assert_eq!((mid.width(), mid.height()), (24, 18));
    }

    #[test]
    #[cfg(feature = "webp")]
    fn generate_from_bytes_rejects_garbage_and_oversize() {
        use crate::meta::Limits;

        let set = VariantSet::web_standard();
        assert!(set.generate_from_bytes(b"junk").is_err());
        let big = set.generate_from_bytes_with_limits(&[0u8; 64], &Limits::new().max_bytes(16));
        assert!(matches!(big, Err(MediaError::TooLarge { .. })));
    }

    #[test]
    #[cfg(all(feature = "webp", feature = "exif"))]
    fn generate_from_bytes_auto_orients() {
        use crate::testutil;

        let set = VariantSet::new().with(Variant::new(
            "thumb",
            Fit::MaxSide(64),
            OutFormat::WebP(None),
        ));
        // 16x8 source tagged as orientation 8 (rotate 90° CCW to view):
        // thumbs must come out 8 wide × 16 tall.
        let src = testutil::noise_image(16, 8);
        let jpg = crate::encode::encode(&src, &OutFormat::Jpeg(90)).unwrap();
        let app1 = crate::exif::app1_segment(&testutil::jpeg_with_orientation(8))
            .unwrap()
            .unwrap();
        let mut tagged = Vec::new();
        tagged.extend_from_slice(&jpg[..2]);
        tagged.extend_from_slice(&app1);
        tagged.extend_from_slice(&jpg[2..]);

        let out = set.generate_from_bytes(&tagged).unwrap();
        let thumb = image::load_from_memory(&out[0].2).unwrap();
        assert_eq!((thumb.width(), thumb.height()), (8, 16), "must be upright");
    }
}
