// Bench fixtures: in-memory encodes have no failure path.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Criterion benches: parallel variant fan-out vs serial, across source
//! sizes 1×–8× (1024² base).
//!
//! Run: `cargo bench --bench parallel` (the `parallel` feature is on by
//! default; the serial arms always compile).

use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};

use media_kit::encode::OutFormat;
use media_kit::resize::Fit;
use media_kit::testutil::noise_image;
use media_kit::variants::{Variant, VariantSet};

/// One source → five variants (thumbnail/small/medium/large/webp).
fn five_variants() -> VariantSet {
    VariantSet::new()
        .with(Variant::new(
            "thumbnail",
            Fit::MaxSide(200),
            OutFormat::WebP(None),
        ))
        .with(Variant::new(
            "small",
            Fit::Width(400),
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
        .with(Variant::new(
            "webp",
            Fit::Width(1920),
            OutFormat::WebP(Some(80.0)),
        ))
}

fn bench_fanout(c: &mut Criterion) {
    let set = five_variants();
    let mut group = c.benchmark_group("variant fanout 1->5");
    group.sample_size(15);
    group.measurement_time(Duration::from_secs(10));

    for (label, side) in [
        ("1x 1024", 1024u32),
        ("2x 2048", 2048),
        ("4x 4096", 4096),
        ("8x 8192", 8192),
    ] {
        let img = noise_image(side, side);
        let bytes_per_iter = 5; // 5 outputs per iteration (ids)
        group.throughput(Throughput::Elements(bytes_per_iter));
        group.bench_with_input(format!("serial {label}"), &img, |b, img| {
            b.iter(|| set.generate_serial(img).unwrap())
        });
        group.bench_with_input(format!("parallel {label}"), &img, |b, img| {
            b.iter(|| set.generate(img).unwrap())
        });
    }
    group.finish();
}

criterion_group!(benches, bench_fanout);
criterion_main!(benches);
