# Reusable OSS

Surveyed 2026-10-04 (GitHub/crates.io metadata, READMEs, source). Criteria: AGPL-3.0 compatible; meets ADR 0002 (lossless UTF-16 names, no panics, determinism); maintained; dependency weight.

## Decisions

| Area | Choice | Why / caveat |
|---|---|---|
| `$MFT` parsing | `mft` crate (MIT/Apache-2.0) at commit `18b6c05`; decided in ADR 0004 | Most mature. Lossless `Utf16LeStr` names are on `master` (PR #147) but not in 0.7.0. Fixup bounds-check panic issue #129 still open |
| `$J` parsing, Rewind | Own implementation, ported from `ntfs-core` (Apache-2.0) | `ntfs-core` has everything (MFT, USN V2–V4, Rewind, carving) but stores names via `from_utf16_lossy`, has 1 star and a single maintainer. Use it as port source and test oracle; credit in `NOTICE` |
| Sigma engine | `rsigma-eval` (MIT), behind our own adapter; decided in ADR 0003 (Hayabusa's engine as fallback) | All 8 correlation types, `Event` trait, explicit timestamps (`process_event_at`), `explain` traces. 0.x with frequent minor bumps; uses `HashMap`, so we sort output |
| Collector | `ntfs-reader` (MIT/Apache-2.0) `=0.6.0`; decided in ADR 0005 | Reads the raw volume and named streams on Windows; its `unsafe` stays inside the dependency. Sparse `$J` extents skipped correctly (S3); 4Kn untested |
| Baseline seed (v0.1) | VanillaWindowsReference (MIT) | Clean-install file lists with hashes and SDDL for Win10/11/Server. One build per version, no post-update state, no 25H2, may lack hidden files. Replaced by our own CI in v0.3 |
| Rules | SigmaHQ, hayabusa-rules (DRL 1.1) | Keep `author` |
| Driver data | LOLDrivers (Apache-2.0) | — |
| LOLBIN / DLL / binary data | LOLBAS, HijackLibs, winbindex (GPL-3.0) | Ship as AGPL-side data, never inside DRL rules |
| Path oracle | usnjrnl_rewind (MIT, Python) | Reference for Rewind correctness |

## Rejected

| Candidate | Reason |
|---|---|
| `usnjrnl-forensic` | Archived 2026-07 (moved into `ntfs-core`) |
| `ntfs` (ColinFinck) | No release since 2023-06; not forensic-oriented. Its `$UpCase` handling is a useful reference |
| `RustyUsn` | Unmaintained since 2020 |
| `usn-journal-rs` | Reads the live journal via FSCTL, not a collected `$J` |
| `sigma-rust` | No correlation support |
| chainsaw / tau-engine | Own rule format; Sigma support via conversion |

## Supporting crates

`fst`, `zstd`, `chrono` (matches `rsigma-eval` and `ntfs-core`), `string-interner`, `indexmap`, `rayon`, `clap`, `csv`, `ts-rs`, `insta`, `proptest`, `cargo-fuzz`, `criterion`. Viewer: Svelte, `vite-plugin-singlefile`, TanStack Virtual, uPlot. All MIT/Apache/BSD/Unlicense.
