# ADR 0004: `$MFT` parser — `mft` crate at a pinned commit

Status: accepted (2026-10-04, approved by the maintainer)

## Context
Spike S2 (docs/plan.md) had to show whether the `mft` crate's unreleased `master` meets ADR 0002 (lossless UTF-16 names, no panics) for `mft-parse`; the fallback was porting `ntfs-core`. Evidence and commands: [spikes/s2-mft/README.md](../../spikes/s2-mft/README.md). Development is on macOS, so the spike used synthetic 1024- and 4096-byte FILE records.

## Decision
**Adopt `mft` at commit `18b6c05` (git dependency) behind `mft-parse`, until a release contains it.**

| Spike criterion | Result |
|---|---|
| Unpaired surrogates survive | Pass via `as_utf16le_bytes()`; `to_utf8_string()` drops them silently |
| 4Kn records | Pass (synthetic; 512-byte fixup stride) |
| Deleted entries, ADS, resident data | Pass |
| No panic (proptest instead of cargo-fuzz, option b) | Pass: 300M cases over 3 properties in 28.5 min, 0 panics (incl. `utf16-simd` paths) |

**Dependency** (vetting from the H2 step, 2026-10-04): MIT/Apache-2.0; `default-features = false` (drops the `mft_dump` CLI deps); 49 transitive crates; `mft` itself is `#![forbid(unsafe_code)]` with no build script. **`utf16-simd 0.1.1`** (same author, released 2025-12, 75 `unsafe`) is a required dependency but sits only on UTF-8 conversion paths.

**Rules for `mft-parse`**
1. Build `NtfsName` from `Utf16LeStr::as_utf16le_bytes()` only. Never use `to_utf8_string`, `Display` or serde output of names (lossy, and that's the `utf16-simd` path).
2. Treat `valid_fixup == Some(false)` as a `Diagnostic`, not a fatal error.
3. Add cargo-fuzz targets in Phase 1 (nightly), covering the same entry points as the spike's properties.

**Release**: ask the maintainer whether a release containing PR #147 is planned (an internet write, so a human posts it). When a release ships, switch to `version = "=x.y.z"` and drop the git source.

Not chosen: porting `ntfs-core` (more code to own; it stays the fallback if the pinned commit has to change often or `utf16-simd` becomes a concern).

## Consequences
- `deny.toml`: `[sources] allow-git = ["https://github.com/omerbenamram/mft"]` when `mft-parse` is created.
- `cargo vet`: exemptions for `mft` (git) and `utf16-simd`; review `utf16-simd` before exempting.
- `docs/architecture.md` "Built on" for `mft-parse` and `docs/research/oss-reuse.md`: mark decided.
