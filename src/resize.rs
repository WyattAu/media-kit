//! Aspect-preserving and cover-fit resizing.

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

/// Resampling filter. Maps 1:1 onto [`image::imageops::FilterType`].
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
    let ft = FilterType::from(filter);

    match *fit {
        Fit::MaxSide(side) => {
            let scale = f64::from(side) / f64::from(w.max(h));
            if !upscale && scale >= 1.0 {
                return img.clone();
            }
            let (nw, nh) = scale_dims(w, h, scale);
            img.resize_exact(nw.max(1), nh.max(1), ft)
        }
        Fit::Width(target) => {
            if !upscale && w <= target {
                return img.clone();
            }
            let scale = f64::from(target) / f64::from(w);
            let (nw, nh) = scale_dims(w, h, scale);
            img.resize_exact(nw.max(1), nh.max(1), ft)
        }
        Fit::Height(target) => {
            if !upscale && h <= target {
                return img.clone();
            }
            let scale = f64::from(target) / f64::from(h);
            let (nw, nh) = scale_dims(w, h, scale);
            img.resize_exact(nw.max(1), nh.max(1), ft)
        }
        Fit::Cover(tw, th) => {
            let crop = crop_to_fill(w, h, tw, th);
            let cropped = img.crop_imm(crop.x, crop.y, crop.w, crop.h);
            if cropped.width() == tw && cropped.height() == th {
                cropped
            } else {
                cropped.resize_exact(tw, th, ft)
            }
        }
        Fit::Exact(tw, th) => img.resize_exact(tw.max(1), th.max(1), ft),
    }
}

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
}
