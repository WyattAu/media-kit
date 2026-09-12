// iai-callgrind benchmarks run once under Valgrind on fixed inputs; the
// harness measures instruction counts, so there is no "expected failure"
// recovery path — a panic aborts the run visibly, which is what we want.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Deterministic regression gate for the two hottest fixed-cost paths
//! behind the README claims:
//!
//! - `sniff` — magic-byte format detection (fixed cost, input-size
//!   independent for the probes used here);
//! - `resize_exact_backend` — the `fast_image_resize` (SIMD) resample that
//!   the README's ~3–15× speedup table is about, pinned on a small
//!   fixed input so the instruction count stays a usable CI gate.
//!
//! Criterion (`benches/pipeline.rs`, `benches/resize_backends.rs`) remains
//! the wall-clock source; this file is the pass/fail gate. Run locally it
//! needs `valgrind`; without it, compile-check only:
//! `cargo bench --no-run --bench iai_hot_path`.

use std::hint::black_box;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};
use media_kit::resize::Filter;
use media_kit::testutil::noise_image;

// Minimal-but-valid magic-byte probes: sniff is header-only, so 32 bytes
// fix the path without shipping whole files.
const JPEG_PROBE: &[u8] = &[
    0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0x00, 0x01, 0x01, 0x00, 0x00, 0x01,
    0x00, 0x01, 0x00, 0x00, 0xFF, 0xD9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];
const PNG_PROBE: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, b'I', b'H', b'D', b'R',
    0, 0, 0x01, 0x00, 0, 0, 0x01, 0x00, 0x08, 0x02, 0, 0, 0, 0, 0, 0,
];
const SVG_PROBE: &[u8] = b"<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/></svg>";

#[library_benchmark]
#[bench::jpeg(JPEG_PROBE)]
fn sniff_jpeg(bytes: &[u8]) -> Option<media_kit::sniff::Format> {
    black_box(media_kit::sniff::sniff(bytes))
}

#[library_benchmark]
#[bench::png(PNG_PROBE)]
fn sniff_png(bytes: &[u8]) -> Option<media_kit::sniff::Format> {
    black_box(media_kit::sniff::sniff(bytes))
}

#[library_benchmark]
#[bench::svg(SVG_PROBE)]
fn sniff_svg(bytes: &[u8]) -> Option<media_kit::sniff::Format> {
    black_box(media_kit::sniff::sniff(bytes))
}

// Setup must live outside the measured region: build one small noise image
// (64x64) and reuse it. The measured call is one full Lanczos3 resample to
// 32x32 through the active (fast, when the feature is on) backend.
fn setup_resize() -> image::DynamicImage {
    noise_image(64, 64)
}

#[library_benchmark]
#[bench::lanczos3_64(setup = setup_resize)]
fn resize_fast(img: image::DynamicImage) -> image::DynamicImage {
    black_box(media_kit::resize::resize_exact_backend(
        &img,
        32,
        32,
        Filter::Lanczos3,
    ))
}

// The `image`/imageops backend on the identical input: the instruction
// delta against `resize_fast` is the load-independent form of the README's
// "fast-resize speedup" claim (wall-clock ratios are noisy under
// background load; instruction counts are not).
#[library_benchmark]
#[bench::lanczos3_64(setup = setup_resize)]
fn resize_plain(img: image::DynamicImage) -> image::DynamicImage {
    black_box(media_kit::resize::resize_plain_exact(
        &img,
        32,
        32,
        Filter::Lanczos3,
    ))
}

library_benchmark_group!(
    name = iai_hot_path;
    benchmarks =
        sniff_jpeg,
        sniff_png,
        sniff_svg,
        resize_fast,
        resize_plain
);

main!(library_benchmark_groups = iai_hot_path);
