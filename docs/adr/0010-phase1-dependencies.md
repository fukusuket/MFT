# ADR 0010: Phase 1 dependencies

Status: accepted (2026-10-06, approved by the maintainer)

## Context
Phase 1 (docs/plan.md) needs error types, a CLI, CSV output, a baseline set and JSON report data. The plan names `thiserror`, `clap`, `anyhow`, `csv`, `fst` and `serde_json`, plus `deny.toml` `allow-git` for `mft` (ADR 0004). `rsigma-eval` (ADR 0003), `mft` (ADR 0004) and `ntfs-reader` (ADR 0005) are already decided and not re-vetted here. The vetting below follows `.claude/rules/dependencies.md` and ADR 0008. The graph was resolved with `cargo metadata` in a scratch project outside the repo, with nothing built.

## Decision
**Add these crates at exact versions, each in the slice that first uses it.**

| Crate | Version | Used in (slice) | Features |
|---|---|---|---|
| `thiserror` | `=2.0.21` | libraries (P1-1) | default |
| `anyhow` | `=1.0.104` | `cli` only (P1-1) | default |
| `clap` | `=4.6.7` | `cli` (P1-1) | `default-features = false`, `["std", "help", "usage", "error-context", "derive"]` |
| `csv` | `=1.4.0` | `report` (P1-1) | default |
| `fst` | `=0.4.7` | `baseline` (P1-3) | default (no `levenshtein`) |
| `serde_json` | `=1.0.151` | `report` (P1-5) | default |
| `serde` | `=1.0.229` | `report` (P1-5) | `["derive"]` |

| Check | Finding (2026-10-06, crates.io API) |
|---|---|
| Names | All verified on crates.io; repos `dtolnay/thiserror`, `dtolnay/anyhow`, `clap-rs/clap`, `BurntSushi/rust-csv`, `BurntSushi/fst`, `serde-rs/json` |
| Licenses | MIT OR Apache-2.0, or Unlicense/MIT; transitive `ryu` is Apache-2.0 OR BSL-1.0, `unicode-ident` adds Unicode-3.0. All are in the `deny.toml` allow-list |
| Maintainers | `dtolnay` (thiserror, anyhow, serde_json, zmij, syn); `kbknapp` plus the rust-cli and clap-rs teams (clap); `BurntSushi` (csv, fst) |
| History | All published since 2014–2019 with 30 M–1.5 G downloads. Latest releases are 2025–2026, except `fst` 0.4.7 (2021-06): stable, but not updated since |
| Transitive crates | 24 in total. Of these, `proc-macro2`, `quote` and `unicode-ident` are already in `Cargo.lock`. `syn` 3 arrives next to the existing `syn` 2, which `multiple-versions = "warn"` reports |
| New crate to note | `zmij` (float formatting for `serde_json`): created 2025-12, same author as `serde_json` (`dtolnay`) |
| Proc-macros | `thiserror-impl`, `clap_derive`, `serde_derive`. `serde_derive` is pulled by `serde_json` → `serde_core` even without our own `derive` |
| `build.rs` | `anyhow`, `thiserror`, `proc-macro2`, `quote`, `serde`, `serde_core`, `zmij`: run `rustc` for version/feature probes; any files go to `OUT_DIR`. `serde_json`: sets cfgs from target env vars. `fst`: writes two generated lookup tables (tag, CRC32C) to `OUT_DIR`. None touch the network or paths outside `OUT_DIR` |

**`mft` git source** (ADR 0004): add `allow-git = ["https://github.com/omerbenamram/mft"]` to `deny.toml` `[sources]` in P1-1, together with the dependency.

**`serde` with `derive`**: approved for `report` (typed report data). It adds no crates, because `serde` and `serde_derive` are already in the graph through `serde_json`.

Not chosen:
- `clap` builder API without `derive`. It saves only `clap_derive` and `heck`, because `syn` is already present.
- An in-memory sorted `Vec` instead of `fst`. It is the fallback if `fst` stays unmaintained, since the S4 spike showed that a `HashSet` handles 1M paths in seconds. `fst` is kept for a compact baseline file.

## Consequences
- Each slice adds only its own crates. `cargo vet` gets audits imported or exemptions at exact versions in the same change (ADR 0008), and `cargo deny check` must pass.
- `deny.toml`: `allow-git` for `mft` in P1-1.
- `docs/plan.md`: tick "Dependencies for Phase 1" on acceptance.
- `.claude/rules/rust.md` already restricts `anyhow` to `cli`.
