// Bench fixtures: in-memory encodes have no failure path.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Criterion benches: resize backend comparison — plain
//! `image::imageops::resize` vs `fast_image_resize` (SIMD), across source
//! sizes 1×–8× (1024² base) at a fixed 800 px output width.
//!
//! Run: `cargo bench --bench resize_backends --features fast-resize`
//! (without the feature only the `plain` arms are present).

use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};

#[cfg(feature = "fast-resize")]
use media_kit::resize::resize_exact_backend;
use media_kit::resize::{fast_resize_available, resize_plain_exact, Filter};
use media_kit::testutil::noise_image;

fn bench_backends(c: &mut Criterion) {
    let mut group = c.benchmark_group("resize backend 800w lanczos3");
    group.sample_size(15);
    group.measurement_time(Duration::from_secs(10));

    for (label, side) in [
        ("1x 1024", 1024u32),
        ("2x 2048", 2048),
        ("4x 4096", 4096),
        ("8x 8192", 8192),
    ] {
        let img = noise_image(side, side);
        group.bench_function(format!("plain {label}"), |b| {
            b.iter(|| resize_plain_exact(&img, 800, 600, Filter::Lanczos3))
        });
        #[cfg(feature = "fast-resize")]
        group.bench_function(format!("fast {label}"), |b| {
            b.iter(|| resize_exact_backend(&img, 800, 600, Filter::Lanczos3))
        });
    }
    group.finish();
}

fn bench_filter_scaling(#[allow(unused_variables)] c: &mut Criterion) {
    // Filter choice cost on the fast backend (4096² source, 800w out).
    #[cfg(feature = "fast-resize")]
    {
        let img = noise_image(4096, 4096);
        let mut group = c.benchmark_group("resize backend filters 4096->800w");
        group.sample_size(15);
        for filter in [
            Filter::Nearest,
            Filter::Triangle,
            Filter::CatmullRom,
            Filter::Lanczos3,
        ] {
            group.bench_function(format!("{filter:?}"), |b| {
                b.iter(|| resize_exact_backend(&img, 800, 800, filter))
            });
        }
        group.finish();
    }
    let _ = fast_resize_available();
}

criterion_group!(benches, bench_backends, bench_filter_scaling);
criterion_main!(benches);
