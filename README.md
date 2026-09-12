# media-kit

[![docs.rs](https://docs.rs/media-kit/badge.svg)](https://docs.rs/media-kit)
[![crates.io](https://img.shields.io/crates/v/media-kit.svg)](https://crates.io/crates/media-kit)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE)

Full image pipeline for Rust — sniff, meta, resize, encode, composite, variants — with decompression-bomb guards, EXIF auto-orientation, SIMD-accelerated resizing, and parallel variant fan-out.

```toml
[dependencies]
media-kit = "0.2"
```

```rust
use media_kit::pipeline::Pipeline;
use media_kit::resize::{Fit, Filter};
use media_kit::encode::OutFormat;

let webp = Pipeline::new(OutFormat::WebP(None))
    .resize(Fit::MaxSide(200), Filter::Lanczos3)
    .run(&jpeg_bytes)?;
```

## Pipeline

```
            ┌──────────────┐   ┌──────────────┐   ┌──────────────┐
 bytes ───▶ │ sniff        │──▶│ bomb-guard   │──▶│ decode (1×)  │
            │ magic bytes  │   │ Limits       │   └──────┬───────┘
            └──────────────┘   │  bytes+dims  │          ▼
                               └──────────────┘   ┌──────────────┐
                                  EXIF orientation│ auto-orient  │ (exif)
                                  ───────────────▶│ rotate/mirror│
                                                  └──────┬───────┘
                                                         ▼
                                                  ┌──────────────┐
                          ops: resize/overlay ───▶│ apply ops    │
                                                  └──────┬───────┘
                                                         ▼
                    ┌─────────────── resize backend ─────────────┐
                    │ fast-resize: fast_image_resize (SIMD)      │
                    │ otherwise:   image::imageops               │
                    └────────────────────┬───────────────────────┘
                                         ▼
                                  ┌──────────────┐
              N variants ────────▶│ encode (par) │──▶ thumb / medium / large / …
                  (rayon)         └──────────────┘    (EXIF/GPS stripped)
```

## What's inside

| Module | Purpose |
|---|---|
| `sniff` | Magic-byte format detection (`Format::Jpeg/Png/Gif/WebP/Avif/Svg/Pdf`), zero deps |
| `meta` | Header-only `dimensions()`, `Limits` + `enforce_limits()` bomb-guard (byte size + decoded dims) |
| `resize` | `Fit::MaxSide/Width/Height/Cover/Exact`, `Filter` (Nearest..Lanczos3), upscale control, dual backend |
| `encode` | `OutFormat::Jpeg(quality)/Png/Gif/WebP(quality)` → `Vec<u8>` |
| `composite` | Alpha `overlay`, tiled watermarks, `flatten_alpha` |
| `variants` | `VariantSet::web_standard()`, `generate_from_bytes()` — decode once, fan out (parallel via rayon) |
| `pipeline` | `Pipeline::new(out).resize(..).auto_orient(true).run(bytes)` — sniff → limits → decode → orient → ops → encode |
| `exif` *(feature)* | EXIF fields + `Orientation` (all 8 EXIF orientations) + auto-orient + opt-in APP1 preservation |

## What's new in 0.2.0

- **Parallel variants** (`parallel`, default on) — `VariantSet::generate` fans
  resize+encode across the rayon pool; the source is decoded once. 6-core
  measurements (1 source → 5 variants, WebP):

  | Source | serial | parallel | speedup |
  |---|---|---|---|
  | 1024² | 427 ms | 238 ms | 1.8× |
  | 2048² | 5.77 s | 3.15 s | 1.8× |
  | 4096² | 11.9 s | 4.99 s | 2.4× |
  | 8192² | 23.8 s | 7.17 s | 3.3× |

- **EXIF auto-orientation** (`exif`, default on) — `read_orientation()` +
  `Orientation::apply()` handle all 8 EXIF orientations with lossless pixel
  shuffles. `Pipeline::auto_orient(true)` and
  `VariantSet::generate_from_bytes()` rotate pixels upright *before*
  resizing, so every variant comes out upright. All outputs are re-encodes:
  **EXIF/GPS is stripped by default** (privacy); `exif::app1_segment()`
  re-injects the original block into JPEG output when you explicitly want to
  preserve it.
- **Fast resize** (`fast-resize`) — the final resample dispatches to
  `fast_image_resize` (runtime SSE4/AVX2/NEON) for 8-bit pixel formats
  (L8/La8/Rgb8/Rgba8); 16-bit and f32 use the `image` backend. Lanczos3
  downscale to 800 px wide, single core:

  | Source | `image` backend | `fast_image_resize` | speedup |
  |---|---|---|---|
  | 1024² | 133 ms | 8.9 ms | ~15× |
  | 2048² | 443 ms | 50 ms | ~9× |
  | 4096² | 1.27 s | 393 ms | ~3.2× |
  | 8192² | 5.12 s | 1.68 s | ~3.0× |

  Output pixels are near-identical between backends (fixed-point vs float
  arithmetic); dimensions are always identical.
- **Quality tuning on variants** — per-variant `OutFormat::Jpeg(q)` and
  `WebP(Some(q))` (true lossy with `webp-lossy`) flow straight into the
  encoders. Note: `image` 0.25's JPEG encoder has no chroma-subsampling knob
  (fixed 4:2:2) — quality is the only JPEG tunable.

## Features

| Feature | Default | Adds |
|---|---|---|
| `std`, `jpeg`, `png`, `gif`, `webp` | ✔ | Core pipeline + codecs |
| `parallel` | ✔ | rayon-backed parallel variant generation (opt out for embedded) |
| `exif` | ✔ | `kamadak-exif`: EXIF fields, orientation, auto-orient, APP1 preserve |
| `webp-lossy` | — | True lossy WebP via libwebp (`image` 0.25 alone is lossless-only; quality is ignored without this) |
| `avif` | — | AVIF decode/encode |
| `fast-resize` | — | `fast_image_resize` SIMD resampling (≈3–15× on 8-bit, see table) |
| `async` | — | `Pipeline::run_async` via `spawn_blocking` |
| `serde` | — | Serialize/Deserialize for configs (`Limits`, `Fit`, `OutFormat`, `VariantSet`, …) |
| `full` | — | Everything above |

Compile-time note: `fast-resize` (fast_image_resize + bytemuck) is the only
heavy add and is off by default; `parallel` (rayon) and `exif`
(kamadak-exif) are small. `cargo build --no-default-features` pulls 11
crates total.

```rust
// async (feature = "async")
let out = pipeline.run_async(bytes).await?;
```

## Safety

`#![forbid(unsafe_code)]`. Inputs are sniffed and size/dimension limited *before* decode, rejecting decompression bombs (default: 10 MiB input, 8192×8192). (`fast_image_resize` performs its own SIMD internally but the integration code carries no `unsafe`.)

## Benchmarks

Reproduce with:

```sh
cargo bench --bench resize_backends --features fast-resize
cargo bench --bench parallel
```

## License

MIT OR Apache-2.0
