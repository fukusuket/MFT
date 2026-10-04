---
paths:
  - "crates/**/*.rs"
  - "spikes/**/*.rs"
---

# Rust code

- Never panic on evidence input: no `unwrap`/`expect`/`panic!` outside tests; `checked_*` for offsets; `try_from`, not `as`; cap allocations derived from input.
- `unsafe` is forbidden (only a collector FFI module may be exempted by ADR).
- Evidence names are `NtfsName` (raw UTF-16). Never key on `from_utf16_lossy` output. Never use `std::path::Path` for evidence paths; compare via `NormPath`.
- Time is UTC `Filetime`; time zones only in `report`/`cli`.
- Output-relevant collections are `BTreeMap`/`IndexMap` or explicitly sorted. Clock and randomness come only from an injected context.
- Fatal problems are `Error`; per-record problems are `Diagnostic` (code + offset, no prose).
- No detection logic or scoring in Rust; expose facts, let Sigma rules decide.
- File names are attacker input: escape in HTML, neutralize CSV formulas, strip control chars in terminal output.
- `thiserror` in libraries, `anyhow` only in `cli`.

# Tests

- Build NTFS records with a test builder; keep `testdata/` files ≤ 1 MB and free of real evidence.
- Parsers: valid, boundary, corrupt (fixup mismatch, truncation, bad lengths), 4Kn; proptest that arbitrary bytes never panic; a fuzz target per new parser entry point.
- Path resolution: deleted parents, reused entries, renames; never invent a path.
- Never delete or `#[ignore]` a test to make CI pass.
