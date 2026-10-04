# ADR 0009: Default `$UpCase` table for `NormPath`

Status: accepted (2026-10-04, approved by the maintainer)

## Context
`NormPath` compares paths case-insensitively the way NTFS does (ADR 0002 #2), which needs the 65,536-entry `$UpCase` table. Our inputs (`$MFT`, `$J`, `$Boot`, `$SDS`) don't include the volume's own `$UpCase`, so we need a built-in default that matches Windows exactly. Approximations are not good enough: a table derived from Unicode simple uppercase (what Rust `char::to_uppercase` gives, Unicode 15.1) differs from Windows at **244 code points** (e.g. U+00B5 `µ`, U+0131 `ı`, U+017F `ſ`). That would make baseline matching disagree with how Windows sees the same names.

## Decision

**Port ntfs-3g's default-table generator into `ntfs-types`.**

| Item | Detail |
|---|---|
| Source | ntfs-3g `libntfs-3g/unistr.c`, `ntfs_upcase_table_build()` (`tuxera/ntfs-3g`, branch `edge`, commit `7f0f841fc52c`). ~150 rows of run/dup/byte tables plus per-version deltas, which rebuild the full table |
| License | GPL-2.0-or-later. Compatible with our AGPL-3.0-only via "or later" (GPLv3 §13). Record source, copyright holders and commit in `NOTICE` (ADR 0001) |
| Version | Windows 7+ table (deltas up to 6.1). ntfs-3g documents the same MD5 for Win8 |
| Verified (2026-10-04) | A Python rebuild of the C tables reproduces all three MD5s that ntfs-3g documents: XP `6fa3db24…`, Vista `2f03b5a6…`, Win7/Win8 `7ff498a4…` |
| Rust shape | `const` table computed at compile time (no build script, no data file), no `unsafe`, no dependency |
| Regression anchor | Test asserts FNV-1a-64 of the little-endian table = `0x48ed4531e9399927` (computed by the verified rebuild; FNV needs no dependency, unlike MD5), plus spot checks (`a→A`, `é→É`, `µ`, `ı` unchanged) |

**Verify against Windows 10/11.** ntfs-3g only documents up to Win8. Add to spike S4 (which already needs a Win11 VM): extract `$UpCase` and check MD5 `7ff498a4…`. If it differs, supersede this ADR with the newer table.

**Not chosen**
- Unicode-derived table: 244 mismatches (above).
- Shipping a `$UpCase` file extracted from Windows: provenance and licensing of Microsoft's file are unclear.
- Reading the evidence volume's own `$UpCase`: most exact, but adds an input. That needs agreement first (AGENTS.md non-negotiables), so it's left for a future ADR.

## Consequences
- `crates/ntfs-types`: upcase table module used by `NormPath` (next plan bullet).
- `NOTICE`: ntfs-3g attribution.
- `docs/plan.md`: S4 "Done when" gains the `$UpCase` MD5 check.
- `deny.toml` unaffected: ported code, not a dependency.
