//! Criterion benches: resize, sniff, webp encode.

use criterion::{criterion_group, criterion_main, Criterion};

use media_kit::encode::{encode, OutFormat};
use media_kit::resize::{resize, Fit, Filter};
use media_kit::sniff::sniff;
use media_kit::testutil::noise_image;

fn bench_resize(c: &mut Criterion) {
    let big = noise_image(4000, 3000);
    c.bench_function("resize lanczos3 4000x3000 -> 800w", |b| {
        b.iter(|| resize(&big, &Fit::Width(800), Filter::Lanczos3))
    });
}

fn bench_sniff(c: &mut Criterion) {
    let big = noise_image(4000, 3000);
    let jpeg_bytes = {
        let rgb = big.to_rgb8();
        let mut out = Vec::new();
        let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85);
        rgb.write_with_encoder(enc).unwrap();
        out
    };
    c.bench_function("sniff 1.4mib jpeg", |b| b.iter(|| sniff(&jpeg_bytes)));
    c.bench_function("sniff tiny header", |b| b.iter(|| sniff(&jpeg_bytes[..32])));
}

fn bench_encode_webp(c: &mut Criterion) {
    let src = noise_image(800, 600);
    c.bench_function("encode webp lossless 800x600", |b| {
        b.iter(|| encode(&src, &OutFormat::WebP(None)))
    });
    c.bench_function("encode png 800x600", |b| b.iter(|| encode(&src, &OutFormat::Png)));
}

criterion_group!(benches, bench_resize, bench_sniff, bench_encode_webp);
criterion_main!(benches);
