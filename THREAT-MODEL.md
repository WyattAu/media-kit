# Threat Model — media-kit

Reference: STRIDE. Scope: the crate's public API surface (`Pipeline`,
`sniff`, `meta::enforce_limits`, `resize`, `encode`, `composite`, `variants`,
`exif`) as used by a downstream service that processes untrusted uploads.
Trust boundaries: (1) untrusted image bytes entering `Pipeline::run` /
`sniff` / decode, (2) the dependency tree (`image` decoders, `imgref`,
`resize`), (3) EXIF metadata parsed from untrusted files.

## Assets

| ID | Asset | Example |
|----|-------|---------|
| A1 | Availability of the caller (no abort, no memory blowup) | Decompression-bomb JPEG exhausts RAM or a decoder panic kills the worker |
| A2 | Confidentiality of image metadata (EXIF) | GPS coordinates or owner identity shipped in resized variants |
| A3 | Correct output format (no format confusion) | A crafted polyglot decoded as a different codec than sniffed |

## STRIDE Analysis

| # | Threat | Category | Surface | Mitigation | Verifying test |
|---|--------|----------|---------|------------|----------------|
| T1 | Decompression bomb (tiny file → giant bitmap) | DoS | `Pipeline::run`, `meta::enforce_limits` | `Limits` (default: 10 MiB bytes, 8192×8192 header dims) enforced **before** decode; violations return `MediaError`, no decode is attempted | `enforce_limits_rejects_11mib`, `defaults` (`src/meta.rs`), `enforce_limits` (`tests/integration.rs`) |
| T2 | Panic on garbage/malformed input | DoS | `sniff`, `Pipeline::run` | `#![forbid(unsafe_code)]`; sniffing is pure byte inspection returning errors; pipeline failures are `Result`; proptest asserts sniff never panics on arbitrary bytes | `sniff_never_panics` (`tests/proptest.rs`), `sniff_rejects_garbage`, `jpeg_magic_is_position_sensitive` |
| T3 | Decoder misidentification (polyglot files) | Spoofing | `sniff` | Sniff is magic-byte based and position-sensitive; actual decode is performed by the `image` crate which re-validates container structure, so a mis-sniffed file fails decode with `MediaError` rather than executing a wrong parser path | `tiny_jpeg_sniffs_as_jpeg`, `tiny_png_sniffs_as_png`, `jpeg_magic_is_position_sensitive` (`tests/proptest.rs`) |
| T4 | Corrupt/truncated images during resize/encode | Tampering | `resize`, `encode`, `composite` | All transforms are `#![forbid(unsafe_code)]` pure functions over validated buffers; errors propagate as `MediaError`; aspect/box properties over arbitrary dims | `width_fit_preserves_aspect`, `cover_exact_box`, `exact_exact_box`, `max_side_idempotent` (`tests/proptest.rs`) |
| T5 | PII/GPS leakage via EXIF in outputs | Info disclosure | `exif` module, `variants` | Mitigation exists but is **opt-in**: `strip_exif` removes metadata; the pipeline does not strip by default for formats carrying metadata. Documented residual risk: callers must strip explicitly | `strip_exif`, `plain_jpeg_has_no_exif`, `read_exif` (`tests/integration.rs`) |
| T6 | Unbounded output amplification | DoS | `variants` | Variants inherit the pipeline's pre-decode `Limits`; each variant re-enters the same limit gate | `web_standard_produces_three_outputs_with_expected_dims`, `accept_within_limits` |

## Repudiation

Not applicable — the crate is a stateless transform library; it keeps no
history and generates no logs.

## Out of Scope

- Decoder memory safety below this crate: the `image` crate's codecs are
  safe Rust, but their resource behavior on adversarial containers is
  bounded here only by the pre-decode `Limits` (byte size + declared header
  dims), not by a pixel-count*bytes-per-pixel product check.
- Content moderation / malicious-image semantics (stegomalware etc.).
- Authenticated uploads: verifying an image's origin is the caller's job.

## Residual Risks

- **R1 (Medium, accepted):** The declared-dimension check trusts header
  fields; a file may declare ≤8192×8192 while its codec expands to more
  (format-dependent). The 10 MiB byte cap is the real bomb brake. Tighten
  `Limits::max_bytes` for untrusted sources.
- **R2 (Medium, accepted):** EXIF is preserved by default (T5). Privacy
  requires an explicit `strip_exif` step in the caller's pipeline.
- **R3 (Low, accepted):** Dependency risk in `image` decoders; a decoder
  advisory would need a patch release. No in-repo `cargo audit` gate.
- **R4 (Low, accepted):** `testutil::tiny_jpeg`/`tiny_png` sample images are
  public fixtures — fine for tests, but they are exported from the crate
  root and add (tiny) surface to the public API.
