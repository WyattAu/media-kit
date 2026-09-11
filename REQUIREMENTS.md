# Requirements — media-kit

Numbered, testable requirements. Every requirement maps to at least one named
test or doc-comment contract; security-relevant items cite THREAT-MODEL.md rows.

Scope: Image pipeline — format sniffing, decode/encode (jpeg/png/gif/webp), resize, variants

## Functional

| ID | Requirement | Priority |
|----|-------------|----------|
| REQ-MD-001 | Sniffing identifies format by magic bytes, never by extension or content trust | MUST |
| REQ-MD-002 | Decode/encode/resize honor configured dimension and byte limits (`Limits`) | MUST |
| REQ-MD-003 | Formats are feature-gated; `--no-default-features` builds compile with typed errors for all format ops | MUST |

## Security

| ID | Requirement | Priority |
|----|-------------|----------|
| REQ-MD-100 | Hostile images cannot cause unbounded allocation: decode limits reject oversized dimensions/bytes before allocation | MUST |
| REQ-MD-101 | Decompression bombs are mitigated by the same limits (declared size validated first) | MUST |

## Observability & API hygiene

| ID | Requirement | Priority |
|----|-------------|----------|
| REQ-MD-900 | All fallible public APIs return typed errors; production `unwrap`/`expect` is denied or explicitly justified with an invariant comment | MUST |
| REQ-MD-901 | Public items carry doc comments with runnable examples where practical | SHOULD |

Reviewed: 2026-09-11
