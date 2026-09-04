#![no_main]

use libfuzzer_sys::fuzz_target;
use media_kit::encode::OutFormat;
use media_kit::meta::{Limits, enforce_limits};
use media_kit::pipeline::Pipeline;
use media_kit::resize::Filter;

/// Cap input size to keep fuzz runs fast.
const MAX_LEN: usize = 64 * 1024;

fuzz_target!(|data: &[u8]| {
    if data.len() < 5 {
        return;
    }
    let bytes = &data[4..data.len().min(MAX_LEN)];
    let b = &data[..4];

    // Arbitrary but bounded Limits: the byte cap can admit or reject the
    // input, and the dimension caps are small enough that anything passing
    // them decodes quickly instead of tripping a decompression bomb.
    let limits = Limits::new()
        .max_bytes(1 + (b[0] as u64) * 16384)
        .max_width(1 + (b[1] as u32) % 512)
        .max_height(1 + (b[2] as u32) % 512);

    // Limit enforcement — Err for violations and unreadable headers, never
    // panic.
    let _ = enforce_limits(bytes, &limits);

    // Full pipeline (sniff -> limits -> decode -> resize -> encode) with
    // fuzz-chosen output format. Decode/encode errors are fine; panics are
    // bugs. Resize dims >= 1 keep the resampler well-defined.
    let out = match b[3] % 4 {
        0 => OutFormat::Jpeg(b[1]),
        1 => OutFormat::Png,
        2 => OutFormat::Gif,
        _ => OutFormat::WebP(None),
    };
    let filter = match b[3] % 5 {
        0 => Filter::Nearest,
        1 => Filter::Triangle,
        2 => Filter::CatmullRom,
        3 => Filter::Gaussian,
        _ => Filter::Lanczos3,
    };
    let side = 1 + (b[2] as u32) % 64;
    let pipeline = Pipeline::new(out)
        .limits(limits)
        .resize(media_kit::resize::Fit::MaxSide(side), filter);
    let _ = pipeline.run(bytes);
});
