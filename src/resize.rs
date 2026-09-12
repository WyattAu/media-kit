//! Aspect-preserving and cover-fit resizing.
//!
//! # Resize backends
//!
//! Two backends implement the final resample step:
//!
//! - **plain** — [`image::imageops::resize`], always available.
//! - **fast** — [`fast_image_resize`] with runtime SIMD dispatch
//!   (SSE4/AVX2/NEON), enabled by the `fast-resize` feature. Used for 8-bit
//!   pixel formats (`L8`, `La8`, `Rgb8`, `Rgba8`) where the buffers can be
//!   handed to the SIMD kernels without conversion; every other format
//!   (16-bit, f32) transparently falls back to the plain backend. Output is
//!   always reconstructed in the source colorspace.
//!
//! Both backends share the same fit/crop logic; only the resample kernel
//! execution differs, so dimensions are identical and pixel values are
//! near-identical (differences stem from fixed-point vs float arithmetic).

use image::imageops::FilterType;
use image::DynamicImage;

/// Target geometry for a resize.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Fit {
    /// Fit within a square of this side, preserving aspect ratio.
    MaxSide(u32),
    /// Fit to this width, height follows aspect ratio.
    Width(u32),
    /// Fit to this height, width follows aspect ratio.
    Height(u32),
    /// Center-crop to fill exactly `(w, h)`, cropping away overflow (avatars).
    Cover(u32, u32),
    /// Stretch to exactly `(w, h)`, ignoring aspect ratio.
    Exact(u32, u32),
}

/// Resampling filter. Maps 1:1 onto [`image::imageops::FilterType`] and,
/// with the `fast-resize` feature, onto `fast_image_resize` filters.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    /// Nearest neighbor — fastest, blocky.
    Nearest,
    /// Linear/triangle — fast, decent.
    Triangle,
    /// Catmull-Rom cubic — good default quality.
    CatmullRom,
    /// Gaussian — smooth.
    Gaussian,
    /// Lanczos3 — highest quality, slowest.
    Lanczos3,
}

impl From<Filter> for FilterType {
    fn from(f: Filter) -> Self {
        match f {
            Filter::Nearest => FilterType::Nearest,
            Filter::Triangle => FilterType::Triangle,
            Filter::CatmullRom => FilterType::CatmullRom,
            Filter::Gaussian => FilterType::Gaussian,
            Filter::Lanczos3 => FilterType::Lanczos3,
        }
    }
}

/// Resize `img` to `fit` with `filter`, never upscaling (a smaller source is
/// returned as-is unless `fit` is `Cover`/`Exact`, which always guarantee
/// their exact target).
#[must_use]
pub fn resize(img: &DynamicImage, fit: &Fit, filter: Filter) -> DynamicImage {
    resize_with(img, fit, filter, false)
}

/// Resize with explicit upscale control.
///
/// When `upscale` is `false`, aspect-preserving fits (`MaxSide`, `Width`,
/// `Height`) leave the image untouched if it is already smaller than the
/// target. `Cover` and `Exact` always produce their exact dimensions.
#[must_use]
pub fn resize_with(img: &DynamicImage, fit: &Fit, filter: Filter, upscale: bool) -> DynamicImage {
    let (w, h) = (img.width(), img.height());

    match *fit {
        Fit::MaxSide(side) => {
            let scale = f64::from(side) / f64::from(w.max(h));
            if !upscale && scale >= 1.0 {
                return img.clone();
            }
            let (nw, nh) = scale_dims(w, h, scale);
            resize_exact_backend(img, nw.max(1), nh.max(1), filter)
        }
        Fit::Width(target) => {
            if !upscale && w <= target {
                return img.clone();
            }
            let scale = f64::from(target) / f64::from(w);
            let (nw, nh) = scale_dims(w, h, scale);
            resize_exact_backend(img, nw.max(1), nh.max(1), filter)
        }
        Fit::Height(target) => {
            if !upscale && h <= target {
                return img.clone();
            }
            let scale = f64::from(target) / f64::from(h);
            let (nw, nh) = scale_dims(w, h, scale);
            resize_exact_backend(img, nw.max(1), nh.max(1), filter)
        }
        Fit::Cover(tw, th) => {
            let crop = crop_to_fill(w, h, tw, th);
            let cropped = img.crop_imm(crop.x, crop.y, crop.w, crop.h);
            if cropped.width() == tw && cropped.height() == th {
                cropped
            } else {
                resize_exact_backend(&cropped, tw, th, filter)
            }
        }
        Fit::Exact(tw, th) => resize_exact_backend(img, tw.max(1), th.max(1), filter),
    }
}

/// Final resample of `img` to exactly `(w, h)`, dispatching to the fastest
/// available backend for the pixel format (see [module docs](self)).
#[must_use]
pub fn resize_exact_backend(img: &DynamicImage, w: u32, h: u32, filter: Filter) -> DynamicImage {
    #[cfg(feature = "fast-resize")]
    {
        if let Some(out) = fast_resize_exact(img, w, h, filter) {
            return out;
        }
    }
    #[allow(unused_variables)]
    plain_resize_exact(img, w, h, filter)
}

/// Force the plain `image::imageops` backend, bypassing dispatch. Exists for
/// A/B benchmarking and as a documented fallback.
#[must_use]
pub fn resize_plain_exact(img: &DynamicImage, w: u32, h: u32, filter: Filter) -> DynamicImage {
    plain_resize_exact(img, w, h, filter)
}

/// `true` when the SIMD-accelerated fast backend is compiled in (the
/// `fast-resize` feature). When `false`, [`resize_exact_backend`] and
/// [`resize_with`] always use the plain backend.
#[must_use]
pub fn fast_resize_available() -> bool {
    cfg!(feature = "fast-resize")
}

fn plain_resize_exact(img: &DynamicImage, w: u32, h: u32, filter: Filter) -> DynamicImage {
    img.resize_exact(w, h, FilterType::from(filter))
}

#[cfg(feature = "fast-resize")]
mod fast {
    use super::Filter;
    use image::DynamicImage;

    pub(super) fn fast_resize_exact(
        img: &DynamicImage,
        w: u32,
        h: u32,
        filter: Filter,
    ) -> Option<DynamicImage> {
        use fast_image_resize::images::Image;
        use fast_image_resize::{PixelType, ResizeOptions, Resizer};

        // 8-bit formats map onto FIR pixel types without conversion; anything
        // else (16-bit, f32) falls back to the plain backend.
        let pixel_type = match img {
            DynamicImage::ImageLuma8(_) => PixelType::U8,
            DynamicImage::ImageLumaA8(_) => PixelType::U8x2,
            DynamicImage::ImageRgb8(_) => PixelType::U8x3,
            DynamicImage::ImageRgba8(_) => PixelType::U8x4,
            _ => return None,
        };
        let options = ResizeOptions::new().resize_alg(fir_alg(filter));
        let mut dst = Image::new(w, h, pixel_type);
        let mut resizer = Resizer::new();
        // Resize errors (e.g. absurd dimensions) fall back to the plain
        // backend so this function keeps its infallible signature.
        resizer.resize(img, &mut dst, &options).ok()?;
        dyn_from_fir_buffer(dst, w, h)
    }

    fn dyn_from_fir_buffer(
        img: fast_image_resize::images::Image,
        w: u32,
        h: u32,
    ) -> Option<DynamicImage> {
        let buf = img.buffer().to_vec();
        match img.pixel_type() {
            fast_image_resize::PixelType::U8 => {
                image::GrayImage::from_raw(w, h, buf).map(DynamicImage::ImageLuma8)
            }
            fast_image_resize::PixelType::U8x2 => {
                image::GrayAlphaImage::from_raw(w, h, buf).map(DynamicImage::ImageLumaA8)
            }
            fast_image_resize::PixelType::U8x3 => {
                image::RgbImage::from_raw(w, h, buf).map(DynamicImage::ImageRgb8)
            }
            fast_image_resize::PixelType::U8x4 => {
                image::RgbaImage::from_raw(w, h, buf).map(DynamicImage::ImageRgba8)
            }
            _ => None,
        }
    }

    /// Filter mapping onto `fast_image_resize` algorithms.
    pub(super) fn fir_alg(f: Filter) -> fast_image_resize::ResizeAlg {
        use fast_image_resize::{FilterType, ResizeAlg};
        match f {
            Filter::Nearest => ResizeAlg::Nearest,
            Filter::Triangle => ResizeAlg::Convolution(FilterType::Bilinear),
            Filter::CatmullRom => ResizeAlg::Convolution(FilterType::CatmullRom),
            Filter::Gaussian => ResizeAlg::Convolution(FilterType::Gaussian),
            Filter::Lanczos3 => ResizeAlg::Convolution(FilterType::Lanczos3),
        }
    }
}

#[cfg(feature = "fast-resize")]
use fast::fast_resize_exact;

fn scale_dims(w: u32, h: u32, scale: f64) -> (u32, u32) {
    (
        ((f64::from(w) * scale).round() as u32).max(1),
        ((f64::from(h) * scale).round() as u32).max(1),
    )
}

struct Crop {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

/// Largest centered crop of `(w, h)` whose aspect ratio matches `(tw, th)`.
fn crop_to_fill(w: u32, h: u32, tw: u32, th: u32) -> Crop {
    let target_ratio = f64::from(tw) / f64::from(th);
    let src_ratio = f64::from(w) / f64::from(h);
    if src_ratio > target_ratio {
        // Source is wider: keep full height, crop width centered.
        let cw = ((f64::from(h) * target_ratio).round() as u32).clamp(1, w);
        Crop {
            x: (w - cw) / 2,
            y: 0,
            w: cw,
            h,
        }
    } else {
        // Source is taller (or exact): keep full width, crop height centered.
        let ch = ((f64::from(w) / target_ratio).round() as u32).clamp(1, h);
        Crop {
            x: 0,
            y: (h - ch) / 2,
            w,
            h: ch,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(any(feature = "jpeg", feature = "png"))]
    use crate::testutil;
    use image::RgbaImage;

    fn img(w: u32, h: u32) -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(w, h, image::Rgba([255, 0, 0, 255])))
    }

    #[test]
    fn max_side_scales_landscape() {
        let out = resize(&img(800, 400), &Fit::MaxSide(200), Filter::Triangle);
        assert_eq!((out.width(), out.height()), (200, 100));
    }

    #[test]
    fn max_side_scales_portrait() {
        let out = resize(&img(400, 800), &Fit::MaxSide(200), Filter::Triangle);
        assert_eq!((out.width(), out.height()), (100, 200));
    }

    #[test]
    fn max_side_skips_smaller_by_default() {
        let src = img(100, 50);
        let out = resize(&src, &Fit::MaxSide(200), Filter::Triangle);
        assert_eq!((out.width(), out.height()), (100, 50));
    }

    #[test]
    fn max_side_upscales_when_allowed() {
        let out = resize_with(&img(100, 50), &Fit::MaxSide(200), Filter::Triangle, true);
        assert_eq!((out.width(), out.height()), (200, 100));
    }

    #[test]
    fn width_skips_smaller_by_default() {
        let src = img(100, 50);
        let out = resize(&src, &Fit::Width(800), Filter::Triangle);
        assert_eq!((out.width(), out.height()), (100, 50));
    }

    #[test]
    fn width_scales_when_larger() {
        let out = resize(&img(1600, 800), &Fit::Width(800), Filter::Triangle);
        assert_eq!((out.width(), out.height()), (800, 400));
    }

    #[test]
    fn width_upscales_when_allowed() {
        let out = resize_with(&img(100, 50), &Fit::Width(800), Filter::Triangle, true);
        assert_eq!((out.width(), out.height()), (800, 400));
    }

    #[test]
    fn height_fit() {
        let out = resize(&img(800, 400), &Fit::Height(100), Filter::Triangle);
        assert_eq!((out.width(), out.height()), (200, 100));
    }

    #[test]
    fn cover_is_exactly_square() {
        let src = img(1000, 400);
        let out = resize(&src, &Fit::Cover(64, 64), Filter::Triangle);
        assert_eq!((out.width(), out.height()), (64, 64));
    }

    #[test]
    fn cover_wide_source() {
        // 2000x500 into 100x100 -> center 500x500 crop scaled to 100x100.
        let out = resize(&img(2000, 500), &Fit::Cover(100, 100), Filter::Triangle);
        assert_eq!((out.width(), out.height()), (100, 100));
    }

    #[test]
    fn cover_taller_than_target() {
        let out = resize(&img(400, 1200), &Fit::Cover(50, 50), Filter::Triangle);
        assert_eq!((out.width(), out.height()), (50, 50));
    }

    #[test]
    fn cover_always_resizes_even_when_smaller() {
        let out = resize(&img(10, 10), &Fit::Cover(64, 64), Filter::Triangle);
        assert_eq!((out.width(), out.height()), (64, 64));
    }

    #[test]
    fn exact_stretches() {
        let out = resize(&img(800, 400), &Fit::Exact(100, 200), Filter::Triangle);
        assert_eq!((out.width(), out.height()), (100, 200));
    }

    #[test]
    fn tiny_source_never_collapses_to_zero() {
        let out = resize_with(&img(1, 1), &Fit::MaxSide(1), Filter::Triangle, true);
        assert_eq!((out.width(), out.height()), (1, 1));
    }

    #[test]
    fn filter_mapping_compiles_for_all_variants() {
        for f in [
            Filter::Nearest,
            Filter::Triangle,
            Filter::CatmullRom,
            Filter::Gaussian,
            Filter::Lanczos3,
        ] {
            let out = resize(&img(100, 100), &Fit::MaxSide(50), f);
            assert_eq!(out.width(), 50);
        }
    }

    #[test]
    #[cfg(feature = "jpeg")]
    fn resize_roundtrip_via_jpeg_source() {
        let jpg = testutil::tiny_jpeg();
        let decoded = image::load_from_memory(&jpg).unwrap();
        let out = resize(&decoded, &Fit::Width(4), Filter::Lanczos3);
        assert_eq!(out.width(), 4);
    }

    // --- fast-resize backend (feature = "fast-resize") ---

    #[test]
    #[cfg(feature = "fast-resize")]
    fn fast_backend_matches_plain_dims() {
        let src = testutil::noise_image(1000, 600);
        for (w, h) in [(200u32, 150u32), (320, 192), (30, 90), (600, 360)] {
            let plain = plain_resize_exact(&src, w, h, Filter::Lanczos3);
            let out = resize_exact_backend(&src, w, h, Filter::Lanczos3);
            assert_eq!((out.width(), out.height()), (plain.width(), plain.height()));
        }
    }

    #[test]
    #[cfg(feature = "fast-resize")]
    fn fast_vs_plain_within_tolerance() {
        let src = testutil::noise_image(512, 384);
        for filter in [Filter::Triangle, Filter::CatmullRom, Filter::Lanczos3] {
            let plain = plain_resize_exact(&src, 200, 150, filter);
            let fast = fast::fast_resize_exact(&src, 200, 150, filter)
                .expect("rgba8 must take the fast path");
            assert_eq!((fast.width(), fast.height()), (200, 150));
            let a = fast.to_rgb8();
            let b = plain.to_rgb8();
            let diff: u64 = a
                .pixels()
                .zip(b.pixels())
                .map(|(p, q)| {
                    i64::from(p[0]).abs_diff(i64::from(q[0]))
                        + i64::from(p[1]).abs_diff(i64::from(q[1]))
                        + i64::from(p[2]).abs_diff(i64::from(q[2]))
                })
                .sum();
            let mean = diff as f64 / f64::from(200 * 150 * 3);
            assert!(mean <= 2.0, "{filter:?}: mean abs diff {mean:.3} > 2.0");
        }
    }

    #[test]
    #[cfg(feature = "fast-resize")]
    fn fast_preserves_colorspace() {
        // Luma8 stays Luma8 through the fast path.
        let gray = DynamicImage::ImageLuma8(image::GrayImage::from_fn(64, 48, |x, y| {
            image::Luma([((x * 3 + y) % 256) as u8])
        }));
        let out = resize(&gray, &Fit::Width(20), Filter::CatmullRom);
        assert!(matches!(out, DynamicImage::ImageLuma8(_)));
        assert_eq!(out.width(), 20);

        let rgba = testutil::noise_image(40, 30);
        let out = resize(&rgba, &Fit::Width(10), Filter::Lanczos3);
        assert!(matches!(out, DynamicImage::ImageRgba8(_)));
    }

    #[test]
    #[cfg(feature = "fast-resize")]
    fn fast_upscale_exact_works() {
        let src = testutil::noise_image(16, 16);
        let out = resize_with(&src, &Fit::Exact(64, 64), Filter::Lanczos3, false);
        assert_eq!((out.width(), out.height()), (64, 64));
    }

    #[test]
    #[cfg(feature = "fast-resize")]
    fn fast_filter_mapping_compiles_for_all_variants() {
        let src = testutil::noise_image(100, 100);
        for f in [
            Filter::Nearest,
            Filter::Triangle,
            Filter::CatmullRom,
            Filter::Gaussian,
            Filter::Lanczos3,
        ] {
            let out = resize_exact_backend(&src, 50, 50, f);
            assert_eq!((out.width(), out.height()), (50, 50));
        }
    }
}
