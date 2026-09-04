#![no_main]

use libfuzzer_sys::fuzz_target;
use media_kit::meta::dimensions;
use media_kit::sniff::{Format, is_image, sniff};

/// Cap input size to keep fuzz runs fast: sniffing and header parsing only
/// look at the first few dozen bytes, and full 64 KiB headers are plenty.
const MAX_LEN: usize = 64 * 1024;

fuzz_target!(|data: &[u8]| {
    let bytes = &data[..data.len().min(MAX_LEN)];

    // Header parsers must return Option/bool on arbitrary bytes, never panic.
    let format: Option<Format> = sniff(bytes);
    let _ = is_image(bytes);

    // Header-only dimension read — Option, no pixel decode, no panic.
    let _ = dimensions(bytes);

    // Extensions are total on all Format variants.
    if let Some(f) = format {
        let _ = f.extension();
    }
});
