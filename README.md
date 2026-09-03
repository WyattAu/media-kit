# media-kit

Full image pipeline for Rust — sniff, meta, resize, encode, composite, variants — with decompression-bomb guards and async support.

```toml
[dependencies]
media-kit = "0.1"
```

```rust
use media_kit::pipeline::Pipeline;
use media_kit::resize::{Fit, Filter};
use media_kit::encode::OutFormat;

let webp = Pipeline::new(OutFormat::WebP(None))
    .resize(Fit::MaxSide(200), Filter::Lanczos3)
    .run(&jpeg_bytes)?;
```

## What's inside

| Module | Purpose |
|---|---|
| `sniff` | Magic-byte format detection (`Format::Jpeg/Png/Gif/WebP/Avif/Svg/Pdf`), zero deps |
| `meta` | Header-only `dimensions()`, `Limits` + `enforce_limits()` bomb-guard (byte size + decoded dims) |
| `resize` | `Fit::MaxSide/Width/Height/Cover/Exact`, `Filter` (Nearest..Lanczos3), upscale control |
| `encode` | `OutFormat::Jpeg(quality)/Png/Gif/WebP(quality)` → `Vec<u8>` |
| `composite` | Alpha `overlay`, tiled watermarks, `flatten_alpha` |
| `variants` | `VariantSet::web_standard()` → thumb/medium/large in one call |
| `pipeline` | `Pipeline::new(out).resize(..).run(bytes)` — sniff → limits → decode → ops → encode |
| `exif` *(feature)* | Read camera/date/GPS via `kamadak-exif`; re-encode strips all metadata |

## Features

- Default: `std`, `jpeg`, `png`, `gif`, `webp`
- `webp-lossy` — true lossy WebP via libwebp (`image` 0.25 alone is lossless-only; quality is ignored without this feature)
- `avif` — AVIF decode/encode
- `exif` — EXIF extraction
- `fast-resize` — `fast_image_resize` integration
- `async` — `Pipeline::run_async` via `spawn_blocking`
- `serde` — Serialize/Deserialize for configs (`Limits`, `Fit`, `OutFormat`, `VariantSet`, …)
- `full` — everything above

```rust
// async (feature = "async")
let out = pipeline.run_async(bytes).await?;
```

## Safety

`#![forbid(unsafe_code)]`. Inputs are sniffed and size/dimension limited *before* decode, rejecting decompression bombs (default: 10 MiB input, 8192×8192).

## License

MIT OR Apache-2.0
