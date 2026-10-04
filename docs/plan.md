# Plan

Current phase: **0**. Tick items as they land (same change). Every item states **Done when** (checkable) and **Out of scope**; agents do nothing outside it. Flow per item: H1 tests approved → TDD cycles on a branch → H3 human review and merge (AGENTS.md). Scope: [product.md](product.md). Design: [architecture.md](architecture.md).

## Phases

| Phase | Scope | Gate (ticked by a human, H4) | Estimate |
|---|---|---|---|
| **0** | Spikes S1–S4, `ntfs-types` | ADR 0003–0006 accepted; AGENTS.md, `deny.toml`, `NOTICE` updated | 1–1.5 wk |
| 1 (v0.1) | Walking skeleton: `$MFT` → baseline (VanillaWindowsReference Win11 24H2) → a few Sigma rules → minimal HTML + CSV | Outside-baseline < 10 % on a real host; 1 GB `$MFT` in ≤ 1 min, ≤ 2 GB RAM | 2–3 wk |
| 2 (v0.2) | USN + Rewind (ported from `ntfs-core`), facts, time window, ~120 SigmaHQ + own rules, Svelte viewer (Summary, Findings, Outside-baseline) | A non-expert decides the next step from a report; Rewind resolution ≥ usnjrnl_rewind | 4–6 wk |
| 3 (v0.3, MVP) | `collect`, own baseline CI, Win10 22H2 / Win11 25H2, correlations, Timeline, File detail, en/ja, accuracy CI | MVP success criteria in [product.md §4](product.md#4-mvp) | 6–8 wk |

## Phase 0

Run spikes with the `/spike` skill. Spike code lives in `spikes/<name>/` and is never moved into `crates/`. Spike dependencies are H2 decisions too ([ADR 0007](adr/0007-security-and-supply-chain.md) P5).

- [x] **`cargo-vet` init** with the first dependency added to the workspace (ADR 0007 P5, ADR 0008)
  - Done when: `cargo vet` passes in CI and is listed in AGENTS.md verify commands.
  - Out of scope: auditing spike-only crates.

- [ ] **S1 `rsigma-eval`** → ADR 0003
  - Done when: `Event` implemented directly on an NTFS event type (no JSON); ~120 SigmaHQ `file_*` rules × 1M events ≤ 10 s; `temporal_ordered` works via `process_event_at` with USN times; identical output on two runs after sorting; `explain` usable as finding evidence; routing by `service: baseline_outside`.
  - Out of scope: real NTFS parsing (use synthetic events), report output.
  - Fallback: extract Hayabusa's engine.
- [ ] **S2 `mft` crate (`master`)** → ADR 0004
  - Done when: unpaired surrogates survive via `Utf16LeStr`; 4Kn records parse; 30 min `cargo fuzz` without panic (issue #129); deleted entries, ADS, resident data readable. Ask the maintainer about a release.
  - Out of scope: path building, USN, wrapping it as `mft-parse`.
  - Fallback: pin git commit → fork → port from `ntfs-core`.
- [ ] **S3 `ntfs-reader`** (Windows, admin) → ADR 0005
  - Done when: raw `$MFT`; `$UsnJrnl:$J` without the sparse region; `$Secure:$SDS`, `$Boot`; no `unsafe` in our code; 4Kn (record as a known limit if untested).
  - Out of scope: zip packaging, `meta.json`, CLI, code signing.
  - Fallback: `std::fs::File` on `\\.\C:` + aligned reader + `ntfs-core` `NtfsFs`.
- [ ] **S4 VanillaWindowsReference → `fst`** → `docs/research/baseline-poc.md`
  - Done when: outside-baseline < 10 % on a real Win11 24H2 `$MFT`; list what VWR misses (hidden, `$` files, ADS) against a clean VM.
  - Out of scope: other Windows builds, baseline CI, distribution format.
  - Fallback: more normalization rules, or bring the baseline CI forward.
- [ ] **ADR 0006**: GPL-3.0 data (LOLBAS, HijackLibs, winbindex) is AGPL-side data matched at runtime, never compiled into DRL rules.
  - Done when: ADR accepted and consistent with `.claude/rules/dependencies.md`.
  - Out of scope: importing the data.
- [ ] **`ntfs-types`** (independent of spikes; one TDD cycle per bullet)
  - [x] `FileRef`: 48-bit entry + 16-bit sequence; `u64` round-trip
  - [x] `Filetime`: `u64` newtype (UTC, 100 ns since 1601); ordering; tests for 0, `u64::MAX`. Unix-time conversion deferred until a consumer needs it
  - [x] `NtfsName`: `Box<[u16]>`; escaped display; proptest round-trip of any `u16` sequence
  - [ ] `NormPath`: built-in default `$UpCase`; case folding incl. non-ASCII
  - [ ] Make the CI `coverage` job blocking (remove `continue-on-error`)
  - Done when: `cargo nextest run -p ntfs-types` and `cargo llvm-cov --workspace --fail-under-lines 90` pass.
  - Out of scope: serde, string parsing, time-zone conversion, path normalization beyond `$UpCase` (belongs to `baseline`).

Inputs needed: a Win11 24H2 `$MFT` (S4); a Windows host with admin rights (S3); a 4Kn disk/image or synthesized records (S2, S3).

## Risks

| Risk | Fallback |
|---|---|
| `mft` fix never released | Pin git commit → fork → port from `ntfs-core` |
| `rsigma-eval` API churn | Pin `=0.x.y` behind the adapter; switch to Hayabusa's engine if costly |
| `ntfs-reader` cannot skip the `$J` sparse region | `ntfs-core` `NtfsFs` approach |
| VanillaWindowsReference lacks hidden/system files | Supplement with one clean-VM `$MFT` in Phase 1 |
| Drift from ported `ntfs-core` code | Record source commit in `NOTICE`; review upstream periodically |
