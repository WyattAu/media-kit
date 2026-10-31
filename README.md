Full image pipeline for Rust; sniff, meta, resize, encode, composite, variants; with decompression-bomb guards, EXIF auto-orientation, SIMD-accelerated resizing, and parallel variant fan-out.
| `variants` | `VariantSet::web_standard()`, `generate_from_bytes()`; decode once, fan out (parallel via rayon) |
| `pipeline` | `Pipeline::new(out).resize(..).auto_orient(true).run(bytes)`; sniff → limits → decode → orient → ops → encode |
- **Parallel variants** (`parallel`, default on); `VariantSet::generate` fans
- **EXIF auto-orientation** (`exif`, default on); `read_orientation()` +
- **Fast resize** (`fast-resize`); the final resample dispatches to
- **Quality tuning on variants**; per-variant `OutFormat::Jpeg(q)` and
  (fixed 4:2:2); quality is the only JPEG tunable.
| `webp-lossy` |; | True lossy WebP via libwebp (`image` 0.25 alone is lossless-only; quality is ignored without this) |
| `avif` |; | AVIF decode/encode |
| `fast-resize` |; | `fast_image_resize` SIMD resampling (≈3–15× on 8-bit, see table) |
| `async` |; | `Pipeline::run_async` via `spawn_blocking` |
| `serde` |; | Serialize/Deserialize for configs (`Limits`, `Fit`, `OutFormat`, `VariantSet`, …) |
| `full` |; | Everything above |
clean machine; re-run when load < nproc). Every claim is inventoried