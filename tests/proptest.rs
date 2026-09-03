//! Property-based tests: invariants over arbitrary inputs.

use proptest::prelude::*;

// Debug-mode image ops are slow; keep case count modest.
const CASES: u32 = 32;

use media_kit::resize::{resize_with, Fit, Filter};
use media_kit::sniff::{self, Format};
use media_kit::testutil::tiny_jpeg;

prop_compose! {
    fn arb_dims()(w in 1u32..=4096, h in 1u32..=4096) -> (u32, u32) {
        (w, h)
    }
}

fn arb_image(w: u32, h: u32) -> image::DynamicImage {
    // Small deterministic fill; dims are what we're testing.
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        w,
        h,
        image::Rgba([12, 34, 56, 255]),
    ))
}

proptest! {
    #![proptest_config(proptest::test_runner::Config { cases: CASES, ..proptest::test_runner::Config::default() })]

    /// Width fit: output width is exactly the target (when downscaling) and
    /// height shrinks proportionally; nothing explodes for arbitrary sizes.
    #[test]
    fn width_fit_preserves_aspect(w in 1u32..=1024, h in 1u32..=1024, target in 1u32..=256) {
        prop_assume!(w > target);
        let out = resize_with(&arb_image(w, h), &Fit::Width(target), Filter::Nearest, false);
        prop_assert_eq!(out.width(), target);
        let want_h = ((f64::from(h) * f64::from(target) / f64::from(w)).round() as u32).max(1);
        prop_assert_eq!(out.height(), want_h);
        prop_assert!(out.width() >= 1 && out.height() >= 1);
    }

    /// MaxSide fit: neither side exceeds the cap, dims stay positive, and
    /// applying it again is a no-op (idempotence for downscale fits).
    #[test]
    fn max_side_idempotent(w in 1u32..=1024, h in 1u32..=1024, side in 1u32..=256) {
        prop_assume!(w.max(h) > side);
        let once = resize_with(&arb_image(w, h), &Fit::MaxSide(side), Filter::Nearest, false);
        prop_assert!(once.width() <= side && once.height() <= side);
        prop_assert!(once.width() >= 1 && once.height() >= 1);
        let twice = resize_with(&once, &Fit::MaxSide(side), Filter::Nearest, false);
        prop_assert_eq!((once.width(), once.height()), (twice.width(), twice.height()));
    }

    /// Cover always produces exactly the requested box for any source.
    #[test]
    fn cover_exact_box(w in 1u32..=1024, h in 1u32..=1024, tw in 1u32..=128, th in 1u32..=128) {
        let out = resize_with(&arb_image(w, h), &Fit::Cover(tw, th), Filter::Nearest, false);
        prop_assert_eq!((out.width(), out.height()), (tw, th));
    }

    /// Exact always produces exactly the requested box.
    #[test]
    fn exact_exact_box(w in 1u32..=1024, h in 1u32..=1024, tw in 1u32..=128, th in 1u32..=128) {
        let out = resize_with(&arb_image(w, h), &Fit::Exact(tw, th), Filter::Nearest, false);
        prop_assert_eq!((out.width(), out.height()), (tw, th));
    }

    /// Sniff never panics and never misidentifies our own fixtures.
    #[test]
    fn sniff_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..4096)) {
        let _ = sniff::sniff(&bytes);
        let _ = sniff::is_image(&bytes);
    }

    /// Sniff on a valid JPEG prefixed with arbitrary junk still identifies
    /// only when the prefix preserves the magic (i.e. never for non-empty
    /// prefixes) — the key property: no panic, and real magic at offset 0 wins.
    #[test]
    fn jpeg_magic_is_position_sensitive(prefix in proptest::collection::vec(any::<u8>(), 1..64)) {
        let jpg = tiny_jpeg();
        let mut mangled = prefix.clone();
        mangled.extend_from_slice(&jpg);
        // Junk prefix hides the magic -> not a JPEG at offset 0.
        if mangled[..3] != [0xFF, 0xD8, 0xFF] {
            prop_assert_ne!(sniff::sniff(&mangled), Some(Format::Jpeg));
        }
        // Original still sniffs.
        prop_assert_eq!(sniff::sniff(&jpg), Some(Format::Jpeg));
    }
}
