//! Layering, watermarks, and alpha flattening.

use image::DynamicImage;
use image::Rgba;
use image::RgbaImage;

use crate::resize::Filter;

/// How an overlay interacts with the layer below.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Blend {
    /// Alpha composite the overlay over the base with the given opacity in
    /// `[0.0, 1.0]` (1.0 = as-is, respecting the overlay's own alpha).
    Over {
        /// Global opacity multiplier for the overlay.
        opacity: f32,
    },
    /// Composite the overlay onto an opaque background color first, then
    /// paste it opaquely (no alpha blending at paste time).
    Flatten {
        /// Opaque background applied to the overlay's transparent pixels.
        background: Rgb8,
    },
}

/// 8-bit RGB color.
pub type Rgb8 = [u8; 3];

/// Draw `overlay_img` onto `base` at `(x, y)` (top-left corner; may be
/// negative to hang off the edge) using `blend`. Returns a new image.
#[must_use]
pub fn overlay(
    base: &DynamicImage,
    overlay_img: &DynamicImage,
    x: i64,
    y: i64,
    blend: Blend,
) -> DynamicImage {
    let mut out = base.to_rgba8();
    let prep = prepare_overlay(overlay_img, blend);
    paste(&mut out, &prep, x, y, blend);
    DynamicImage::ImageRgba8(out)
}

/// Tile `overlay_img` across `base` at the given opacity (tiled watermark).
#[must_use]
pub fn tile(base: &DynamicImage, overlay_img: &DynamicImage, opacity: f32) -> DynamicImage {
    let mut out = base.to_rgba8();
    let prep = prepare_overlay(overlay_img, Blend::Over { opacity });
    let (ow, oh) = (prep.width() as i64, prep.height() as i64);
    let (bw, bh) = (out.width() as i64, out.height() as i64);
    if ow == 0 || oh == 0 {
        return DynamicImage::ImageRgba8(out);
    }
    // Step by the overlay size (gap-free tiling), starting from top-left.
    let mut y = 0;
    while y < bh {
        let mut x = 0;
        while x < bw {
            paste(&mut out, &prep, x, y, Blend::Over { opacity });
            x += ow;
        }
        y += oh;
    }
    DynamicImage::ImageRgba8(out)
}

/// Composite an RGBA image onto an opaque background color, dropping alpha.
#[must_use]
pub fn flatten_alpha(img: &DynamicImage, bg: Rgb8) -> DynamicImage {
    let rgba = img.to_rgba8();
    let mut out = RgbaImage::new(rgba.width(), rgba.height());
    for (dst, src) in out.pixels_mut().zip(rgba.pixels()) {
        let a = f32::from(src.0[3]) / 255.0;
        let blend = |s: u8, b: u8| -> u8 {
            let v = f32::from(s) * a + f32::from(b) * (1.0 - a);
            v.round().clamp(0.0, 255.0) as u8
        };
        *dst = Rgba([
            blend(src.0[0], bg[0]),
            blend(src.0[1], bg[1]),
            blend(src.0[2], bg[2]),
            255,
        ]);
    }
    DynamicImage::ImageRgba8(out)
}

/// Overload usable in `resize` chains: convenience re-export for callers that
/// want to shrink an overlay before pasting.
#[must_use]
pub fn resized_overlay(
    overlay_img: &DynamicImage,
    fit: &crate::resize::Fit,
    filter: Filter,
) -> DynamicImage {
    crate::resize::resize(overlay_img, fit, filter)
}

fn prepare_overlay(overlay_img: &DynamicImage, blend: Blend) -> RgbaImage {
    match blend {
        Blend::Over { opacity } => {
            let rgba = overlay_img.to_rgba8();
            let mut out = rgba.clone();
            for p in out.pixels_mut() {
                let a = (f32::from(p.0[3]) * opacity.clamp(0.0, 1.0))
                    .round()
                    .clamp(0.0, 255.0) as u8;
                p.0[3] = a;
            }
            out
        }
        Blend::Flatten { background } => {
            let flat = flatten_alpha(overlay_img, background);
            flat.to_rgba8()
        }
    }
}

fn paste(base: &mut RgbaImage, over: &RgbaImage, x: i64, y: i64, blend: Blend) {
    let (bw, bh) = (base.width() as i64, base.height() as i64);
    for oy in 0..over.height() as i64 {
        for ox in 0..over.width() as i64 {
            let px = x + ox;
            let py = y + oy;
            if px < 0 || py < 0 || px >= bw || py >= bh {
                continue;
            }
            let src = over.get_pixel(ox as u32, oy as u32);
            let dst = base.get_pixel_mut(px as u32, py as u32);
            match blend {
                Blend::Flatten { .. } => {
                    *dst = *src;
                }
                Blend::Over { .. } => blend_pixel(dst, src),
            }
        }
    }
}

/// Standard source-over alpha compositing of `src` onto `dst` (both opaque
/// buffers holding premultiplied-by-alpha results in RGB).
fn blend_pixel(dst: &mut Rgba<u8>, src: &Rgba<u8>) {
    let sa = f32::from(src.0[3]) / 255.0;
    let da = f32::from(dst.0[3]) / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a <= 0.0 {
        *dst = Rgba([0, 0, 0, 0]);
        return;
    }
    let mix = |s: u8, d: u8| -> u8 {
        let sv = f32::from(s) / 255.0;
        let dv = f32::from(d) / 255.0;
        let v = (sv * sa + dv * da * (1.0 - sa)) / out_a;
        (v * 255.0).round().clamp(0.0, 255.0) as u8
    };
    *dst = Rgba([
        mix(src.0[0], dst.0[0]),
        mix(src.0[1], dst.0[1]),
        mix(src.0[2], dst.0[2]),
        (out_a * 255.0).round().clamp(0.0, 255.0) as u8,
    ]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;
    use image::RgbaImage;

    fn solid(w: u32, h: u32, c: [u8; 4]) -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(w, h, Rgba(c)))
    }

    #[test]
    fn overlay_opaque_covers_base() {
        let base = solid(10, 10, [255, 0, 0, 255]);
        let over = solid(4, 4, [0, 0, 255, 255]);
        let out = overlay(&base, &over, 3, 3, Blend::Over { opacity: 1.0 });
        assert_eq!(out.get_pixel(4, 4).0, [0, 0, 255, 255]);
        assert_eq!(out.get_pixel(0, 0).0, [255, 0, 0, 255]);
    }

    #[test]
    fn overlay_negative_offset_clips() {
        let base = solid(10, 10, [255, 0, 0, 255]);
        let over = solid(4, 4, [0, 255, 0, 255]);
        let out = overlay(&base, &over, -2, -2, Blend::Over { opacity: 1.0 });
        assert_eq!(out.get_pixel(0, 0).0, [0, 255, 0, 255]);
        assert_eq!(out.get_pixel(1, 1).0, [0, 255, 0, 255]);
        assert_eq!(out.get_pixel(2, 2).0, [255, 0, 0, 255]);
        // No panic, dims preserved.
        assert_eq!((out.width(), out.height()), (10, 10));
    }

    #[test]
    fn overlay_half_opacity_blends() {
        let base = solid(2, 2, [0, 0, 0, 255]);
        let over = solid(2, 2, [255, 255, 255, 255]);
        let out = overlay(&base, &over, 0, 0, Blend::Over { opacity: 0.5 });
        let px = out.get_pixel(0, 0).0;
        assert!(px[0] > 100 && px[0] < 160, "got {:?}", px);
    }

    #[test]
    fn flatten_replaces_alpha() {
        let src = solid(3, 3, [255, 0, 0, 128]);
        let out = flatten_alpha(&src, [0, 0, 255]);
        let px = out.get_pixel(0, 0).0;
        assert_eq!(px[3], 255);
        assert_eq!(px[0], 128, "50% red over blue gives ~128 red");
    }

    #[test]
    fn flatten_transparent_becomes_bg() {
        let src = solid(3, 3, [0, 0, 0, 0]);
        let out = flatten_alpha(&src, [10, 20, 30]);
        assert_eq!(out.get_pixel(0, 0).0, [10, 20, 30, 255]);
    }

    #[test]
    fn tile_covers_everything() {
        let base = solid(50, 30, [255, 0, 0, 255]);
        let over = solid(16, 16, [0, 0, 255, 255]);
        let out = tile(&base, &over, 1.0).to_rgba8();
        // Every pixel must now be blue-ish (opacity 1.0 opaque tile).
        for p in out.pixels() {
            assert_eq!(p.0, [0, 0, 255, 255]);
        }
    }

    #[test]
    fn tile_zero_opacity_is_identity() {
        let base = solid(20, 20, [1, 2, 3, 255]);
        let over = solid(8, 8, [255, 255, 255, 255]);
        let out = tile(&base, &over, 0.0).to_rgba8();
        for p in out.pixels() {
            assert_eq!(p.0, [1, 2, 3, 255]);
        }
    }

    #[test]
    fn blend_flatten_pastes_opaquely() {
        let base = solid(8, 8, [255, 0, 0, 255]);
        // Overlay with alpha hole in the middle.
        let mut over_img = RgbaImage::new(4, 4);
        for p in over_img.pixels_mut() {
            *p = Rgba([0, 0, 255, 0]);
        }
        let over = DynamicImage::ImageRgba8(over_img);
        let out = overlay(
            &base,
            &over,
            0,
            0,
            Blend::Flatten {
                background: [0, 255, 0],
            },
        );
        assert_eq!(out.get_pixel(0, 0).0, [0, 255, 0, 255]);
    }

    #[test]
    fn resized_overlay_helper() {
        let over = solid(100, 100, [9, 9, 9, 255]);
        let r = resized_overlay(&over, &crate::resize::Fit::MaxSide(50), Filter::Triangle);
        assert_eq!((r.width(), r.height()), (50, 50));
    }
}
