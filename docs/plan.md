# Plan

Current phase: **0**. Tick items as they land (same change). Scope: [product.md](product.md). Design: [architecture.md](architecture.md).

## Phases

| Phase | Scope | Gate (done when) | Estimate |
|---|---|---|---|
| **0** | Spikes S1–S4, `ntfs-types` | ADR 0003–0006 accepted; AGENTS.md, `deny.toml`, `NOTICE` updated | 1–1.5 wk |
| 1 (v0.1) | Walking skeleton: `$MFT` → baseline (VanillaWindowsReference Win11 24H2) → a few Sigma rules → minimal HTML + CSV | Outside-baseline < 10 % on a real host; 1 GB `$MFT` in ≤ 1 min, ≤ 2 GB RAM | 2–3 wk |
| 2 (v0.2) | USN + Rewind (ported from `ntfs-core`), facts, time window, ~120 SigmaHQ + own rules, Svelte viewer (Summary, Findings, Outside-baseline) | A non-expert decides the next step from a report; Rewind resolution ≥ usnjrnl_rewind | 4–6 wk |
| 3 (v0.3, MVP) | `collect`, own baseline CI, Win10 22H2 / Win11 25H2, correlations, Timeline, File detail, en/ja, accuracy CI | MVP success criteria in [product.md §4](product.md#4-mvp) | 6–8 wk |

## Phase 0

Run spikes with the `/spike` skill. Spike code lives in `spikes/<name>/` and is never moved into `crates/`.

- [ ] **S1 `rsigma-eval`** → ADR 0003
  - Verify: `Event` implemented directly on an NTFS event type (no JSON); ~120 SigmaHQ `file_*` rules × 1M events ≤ 10 s; `temporal_ordered` works via `process_event_at` with USN times; identical output on two runs after sorting; `explain` usable as finding evidence; routing by `service: baseline_outside`.
  - Fallback: extract Hayabusa's engine.
- [ ] **S2 `mft` crate (`master`)** → ADR 0004
  - Verify: unpaired surrogates survive via `Utf16LeStr`; 4Kn records parse; 30 min `cargo fuzz` without panic (issue #129); deleted entries, ADS, resident data readable. Ask the maintainer about a release.
  - Fallback: pin git commit → fork → port from `ntfs-core`.
- [ ] **S3 `ntfs-reader`** (Windows, admin) → ADR 0005
  - Verify: raw `$MFT`; `$UsnJrnl:$J` without the sparse region; `$Secure:$SDS`, `$Boot`; no `unsafe` in our code; 4Kn (record as a known limit if untested).
  - Fallback: `std::fs::File` on `\\.\C:` + aligned reader + `ntfs-core` `NtfsFs`.
- [ ] **S4 VanillaWindowsReference → `fst`** → `docs/research/baseline-poc.md`
  - Verify: outside-baseline < 10 % on a real Win11 24H2 `$MFT`; list what VWR misses (hidden, `$` files, ADS) against a clean VM.
  - Fallback: more normalization rules, or bring the baseline CI forward.
- [ ] **ADR 0006**: GPL-3.0 data (LOLBAS, HijackLibs, winbindex) is AGPL-side data matched at runtime, never compiled into DRL rules.
- [ ] **`ntfs-types`** (independent of spikes). Done when `cargo nextest run -p ntfs-types` passes with:
  - `NtfsName` (`Box<[u16]>`, escaped display): proptest round-trip of any `u16` sequence
  - `NormPath` (built-in default `$UpCase`): case folding incl. non-ASCII
  - `Filetime` (`u64`, UTC): 0, `u64::MAX`, pre-1601
  - `FileRef` (48-bit entry + 16-bit sequence): `u64` round-trip

Inputs needed: a Win11 24H2 `$MFT` (S4); a Windows host with admin rights (S3); a 4Kn disk/image or synthesized records (S2, S3).

## Risks

| Risk | Fallback |
|---|---|
| `mft` fix never released | Pin git commit → fork → port from `ntfs-core` |
| `rsigma-eval` API churn | Pin `=0.x.y` behind the adapter; switch to Hayabusa's engine if costly |
| `ntfs-reader` cannot skip the `$J` sparse region | `ntfs-core` `NtfsFs` approach |
| VanillaWindowsReference lacks hidden/system files | Supplement with one clean-VM `$MFT` in Phase 1 |
| Drift from ported `ntfs-core` code | Record source commit in `NOTICE`; review upstream periodically |
