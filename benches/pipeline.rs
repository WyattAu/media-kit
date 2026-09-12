// Bench fixtures: in-memory encodes have no failure path.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Criterion benches: resize, sniff, encode. Individual benches are gated on
//! the codec features they need.

use criterion::{criterion_group, criterion_main, Criterion};

use media_kit::encode::{encode, OutFormat};
use media_kit::resize::{resize, Filter, Fit};
use media_kit::testutil::noise_image;

fn bench_resize(c: &mut Criterion) {
    let big = noise_image(4000, 3000);
    c.bench_function("resize lanczos3 4000x3000 -> 800w", |b| {
        b.iter(|| resize(&big, &Fit::Width(800), Filter::Lanczos3))
    });
}

#[cfg(feature = "jpeg")]
fn bench_sniff(c: &mut Criterion) {
    let big = noise_image(4000, 3000);
    let jpeg_bytes = {
        let rgb = big.to_rgb8();
        let mut out = Vec::new();
        let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85);
        rgb.write_with_encoder(enc).unwrap();
        out
    };
    c.bench_function("sniff 1.4mib jpeg", |b| {
        b.iter(|| media_kit::sniff::sniff(&jpeg_bytes))
    });
    c.bench_function("sniff tiny header", |b| {
        b.iter(|| media_kit::sniff::sniff(&jpeg_bytes[..32]))
    });
}

fn bench_encode(c: &mut Criterion) {
    let src = noise_image(800, 600);
    c.bench_function("encode png 800x600", |b| {
        b.iter(|| encode(&src, &OutFormat::Png))
    });
    #[cfg(feature = "webp")]
    c.bench_function("encode webp lossless 800x600", |b| {
        b.iter(|| encode(&src, &OutFormat::WebP(None)))
    });
}

#[cfg(feature = "jpeg")]
criterion_group!(benches, bench_resize, bench_sniff, bench_encode);
#[cfg(not(feature = "jpeg"))]
criterion_group!(benches, bench_resize, bench_encode);
criterion_main!(benches);
