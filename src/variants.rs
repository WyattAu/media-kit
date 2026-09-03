//! Multi-size output generation ("thumb", "medium", "large", ...).

use std::vec::Vec;

use crate::encode::OutFormat;
use crate::error::MediaError;
use crate::resize::{resize, Fit, Filter};

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
            .with(Variant::new("thumb", Fit::MaxSide(200), OutFormat::WebP(None)))
            .with(Variant::new("medium", Fit::Width(800), OutFormat::WebP(None)))
            .with(Variant::new("large", Fit::Width(1920), OutFormat::WebP(None)))
    }

    /// Encode one image into every variant.
    ///
    /// Returns `(name, format, bytes)` tuples in declaration order.
    ///
    /// # Errors
    ///
    /// [`MediaError::Encode`] when any variant fails to encode.
    pub fn generate(
        &self,
        img: &image::DynamicImage,
    ) -> Result<Vec<(String, OutFormat, Vec<u8>)>, MediaError> {
        self.variants
            .iter()
            .map(|v| {
                let resized = resize(img, &v.fit, v.filter);
                let bytes = crate::encode::encode(&resized, &v.format)?;
                Ok((v.name.clone(), v.format, bytes))
            })
            .collect()
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
    fn smaller_source_is_not_upscaled() {
        let set = VariantSet::web_standard();
        let out = set.generate(&img(600, 400)).unwrap();
        let large = image::load_from_memory(&out[2].2).unwrap();
        assert_eq!((large.width(), large.height()), (600, 400));
    }

    #[test]
    fn custom_set_mixed_formats() {
        let set = VariantSet::new()
            .with(Variant::new("avatar", Fit::Cover(64, 64), OutFormat::Jpeg(85)))
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
}
