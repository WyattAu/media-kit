# Changelog

All notable changes to this project are documented here. Format: [Keep a
Changelog](https://keepachangelog.com/) — versions follow [semver](https://semver.org).

## [Unreleased]

## [0.2.0] - 2026-09-12

### Added
- **Parallel variant generation** (`parallel` feature, on by default):
  `VariantSet::generate` fans resize+encode across a rayon thread pool while
  decoding the source exactly once; `generate_serial` forces single-threaded
  execution. Serial and parallel outputs are byte-identical for deterministic
  codecs (PNG/JPEG/WebP-lossless), verified by tests.
- **`VariantSet::generate_from_bytes` / `generate_from_bytes_with_limits`**:
  sniff → bomb-guard → single decode → EXIF auto-orient → variant fan-out in
  one call (recommended for uploads).
- **EXIF orientation** (`exif` feature, now default-on): `Orientation` enum
  covering all 8 EXIF orientation values, `read_orientation()`, and
  `Orientation::apply()` (lossless rotate/mirror). `Pipeline::auto_orient(true)`
  and the byte-level variant API rotate pixels upright before any op, so all
  outputs come out upright.
- **EXIF privacy story**: re-encodes strip all EXIF/GPS by default (now
  documented in the module docs); `exif::app1_segment()` extracts the raw
  EXIF block for opt-in re-injection into JPEG output. `ExifData` gained an
  `orientation` field.
- **Fast resize backend** (`fast-resize` feature): resampling dispatches to
  `fast_image_resize` (runtime SIMD: SSE4/AVX2/NEON) for 8-bit pixel formats
  (L8/La8/Rgb8/Rgba8) with a transparent `image::imageops` fallback for
  16-bit/f32 formats and on any resize error. Public `resize_plain_exact`
  (force plain) and `fast_resize_available()` probes; `Filter` maps onto
  `fast_image_resize` filters (Nearest/Triangle→Bilinear/CatmullRom/Gaussian/
  Lanczos3). Measured ~3–15× faster on Lanczos3 downscales; pixel output
  within tolerance, dimensions identical.
- **Benches**: `benches/resize_backends.rs` (plain vs SIMD across 1×–8×
  source sizes and filters) and `benches/parallel.rs` (1 source → 5 variants,
  serial vs parallel across 1×–8× sizes). README has the numbers.

### Changed
- Default features are now `std, jpeg, png, gif, webp, exif, parallel`
  (previously `std, jpeg, png, gif, webp`). Opt out with
  `default-features = false`.
- `full` now includes `parallel`.
- Module docs document that JPEG chroma subsampling is **not** tunable via
  `image` 0.25 (encoder is fixed at 4:2:2); quality remains the only JPEG
  knob, passed through per variant.

## [0.1.1] - 2026-09-05

### Added
- Initial public release.
