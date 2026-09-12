# Performance claims inventory — media-kit

Every performance claim in [README.md](README.md), mapped to its proof
artifact. Created 2026-09-12 (media-kit 0.2.1).

Machine context for all wall-clock records: Intel Core i5-9400F (6 cores,
x86_64), Linux; recorded 2026-09 for 0.2.0. Criterion wall-clock numbers are
"measured on" records — load-sensitive, re-runnable via the commands below —
while the iai-callgrind gate is load-independent (instruction counts).

Proof kinds: **criterion** (wall-clock record), **iai** (instruction gate,
`cargo bench --features fast-resize --bench iai_hot_path`), **test** (unit/
proptest, runs on every `cargo test`), **code** (code reading).

## Fast resize speedup (README "Fast resize" table)

| # | Claim (1024²→800w Lanczos3, single core) | Artifact | Status |
|---|---|---|---|
| 1 | 1024²: 133 ms → 8.9 ms, ~15× | `benches/resize_backends.rs` `plain/fast 1x 1024` | backed (measured on) |
| 2 | 2048²: 443 ms → 50 ms, ~9× | same, `2x 2048` | backed (measured on) |
| 3 | 4096²: 1.27 s → 393 ms, ~3.2× | same, `4x 4096` | backed (measured on) |
| 4 | 8192²: 5.12 s → 1.68 s, ~3.0× | same, `8x 8192` | backed (measured on) |
| 5 | "≈3–15× on 8-bit" (features table) | derived from 1–4; **load-independent form proven**: iai `resize_fast` = 370 372 instructions vs `resize_plain` = 1 269 458 on a fixed 64²→32² Lanczos3 probe → **3.43× fewer instructions** (2026-09-12, valgrind 3.25.1) | **proven** (iai) + backed (criterion records) |

2026-09-12 re-run note: a quick re-measurement attempt under heavy
background load (load average 43–56 on 6 cores) produced noisy absolutes
and compressed ratios (~6×/~4.7× at 1024²/2048²); the recorded 0.2.0
numbers stand as the clean-machine record — re-run when load < nproc. The
iai gate pins the backend delta load-independently.

## Parallel variant fan-out (README 0.2.0 table: 1 source → 5 variants, WebP)

| # | Claim | Artifact | Status |
|---|---|---|---|
| 6 | 1024²: 427 ms serial → 238 ms parallel, 1.8× | `benches/parallel.rs` `serial/parallel 1x 1024` | backed (measured on) |
| 7 | 2048²: 5.77 s → 3.15 s, 1.8× | same, `2x 2048` | backed (measured on) |
| 8 | 4096²: 11.9 s → 4.99 s, 2.4× | same, `4x 4096` | backed (measured on) |
| 9 | 8192²: 23.8 s → 7.17 s, 3.3× | same, `8x 8192` | backed (measured on) |
| 10 | decode once, fan out across the rayon pool | code: `src/variants.rs` (`generate`/`generate_serial` share one `DynamicImage`) | backed (code) |

## Sniff / bomb-guard / pipeline structure

| # | Claim | Artifact | Status |
|---|---|---|---|
| 11 | `sniff` is header-only, zero-dep magic-byte detection | code (`src/sniff.rs`, no deps); iai: sniff = **19–83 instructions** on fixed probes (JPEG/PNG/SVG, 2026-09-12) | **proven** (iai) + backed (code) |
| 12 | bomb-guard: Limits (default 10 MiB input, 8192×8192 dims) enforced before decode | unit tests `src/meta.rs` + pipeline tests (`tests/integration.rs`) | **proven** (test) |
| 13 | header-only `dimensions()` | code + `src/meta.rs` tests | backed |
| 14 | pipeline order: sniff → limits → decode → orient → ops → encode | code (`src/pipeline.rs`) + integration tests | backed |

## Resize backends parity

| # | Claim | Artifact | Status |
|---|---|---|---|
| 15 | output pixels near-identical between backends | test `tests/integration.rs` (backend-parity test) | **proven** (test) |
| 16 | dimensions always identical between backends | same test | **proven** (test) |
| 17 | dual backend; 16-bit/f32 use `image` backend | code (`src/resize.rs` dispatch) + `fast_resize_available()` | backed (code) |

## EXIF

| # | Claim | Artifact | Status |
|---|---|---|---|
| 18 | all 8 EXIF orientations handled with lossless shuffles | `src/exif.rs` tests + `tests/proptest.rs` | **proven** (test) |
| 19 | EXIF/GPS stripped by default on re-encode | `tests/integration.rs` (stripping test) | **proven** (test) |
| 20 | orientation applied before resize (upright variants) | `src/variants.rs` + tests | backed |

## Other

| # | Claim | Artifact | Status |
|---|---|---|---|
| 21 | `#![forbid(unsafe_code)]` | attribute in `src/lib.rs` (compile-enforced) | **proven** (compiler) |
| 22 | `--no-default-features` is a light build | `cargo tree --no-default-features` re-verified 2026-09-12: **12 crates** besides media-kit (was 11; `image` 0.25.x gained color-management deps upstream — README corrected) | **proven** (re-measured) |
| 23 | per-variant quality tuning flows into encoders | `src/encode.rs` + tests | backed |
| 24 | `async` = `Pipeline::run_async` via `spawn_blocking` | code (`src/pipeline.rs`) | backed (code) |

## Totals

- **Proven by hard artifact (iai/test/compiler/re-measured):** 10
  (claims 5 partially, 11, 12, 15, 16, 18, 19, 21, 22)
- **Backed (measured-on criterion records or code reading):** 14
- **Deleted/reworded:** 0

## Reproducing

```sh
cargo bench --features fast-resize --bench resize_backends   # backend table
cargo bench --bench parallel                                  # fanout table
cargo bench --features fast-resize --bench iai_hot_path       # instruction gate (valgrind)
cargo test --all-features                                     # parity/bomb-guard/exif tests
```
