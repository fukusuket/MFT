# Plan

Current phase: **2**. Tick items as they land (same change). Every item states **Done when** (checkable) and **Out of scope**; agents do nothing outside it. Flow per item: H1 tests approved → TDD cycles on a branch → H3 human review and merge (AGENTS.md). Scope: [product.md](product.md). Design: [architecture.md](architecture.md).

## Phases

| Phase | Scope | Gate (ticked by a human, H4) | Estimate |
|---|---|---|---|
| 0 | Spikes S1–S4, `ntfs-types` | ✅ 2026-10-06. ADR 0003–0006 accepted; AGENTS.md, `deny.toml`, `NOTICE` updated | 1–1.5 wk |
| 1 (v0.1) | Walking skeleton: `$MFT` → baseline (VanillaWindowsReference Win11 24H2) → a few Sigma rules → minimal HTML + CSV | Outside-baseline files in the window < 10 % of all files on a real host; 1 GB `$MFT` in ≤ 1 min, ≤ 2 GB RAM | 2–3 wk |
| **2 (v0.2)** | USN + Rewind, facts, time window, own generic rules + loading external rule sets, viewer (Summary, Findings, Outside-baseline) | A non-expert decides the next step from a report; Rewind resolution ≥ usnjrnl_rewind | 4–6 wk |
| 3 (v0.3, MVP) | `collect`, own baseline CI, Win10 22H2 / Win11 25H2, correlations, Timeline, File detail, en/ja, accuracy CI | MVP success criteria in [product.md §4](product.md#4-mvp) | 6–8 wk |

## Phase 0

Run spikes with the `/spike` skill. Spike code lives in `spikes/<name>/` and is never moved into `crates/`. Spike dependencies are H2 decisions too ([ADR 0007](adr/0007-security-and-supply-chain.md) P5).

- [x] **`cargo-vet` init** with the first dependency added to the workspace (ADR 0007 P5, ADR 0008)
  - Done when: `cargo vet` passes in CI and is listed in AGENTS.md verify commands.
  - Out of scope: auditing spike-only crates.

- [x] **S1 `rsigma-eval`** → ADR 0003
  - Done when: `Event` implemented directly on an NTFS event type (no JSON); ~120 SigmaHQ `file_*` rules × 1M events ≤ 10 s; `temporal_ordered` works via `process_event_at` with USN times; identical output on two runs after sorting; `explain` usable as finding evidence; routing by `service: baseline_outside`.
  - Out of scope: real NTFS parsing (use synthetic events), report output.
  - Fallback: extract Hayabusa's engine.
- [x] **S2 `mft` crate (`master`)** → ADR 0004
  - Done when: unpaired surrogates survive via `Utf16LeStr`; 4Kn records parse; 30 min `cargo fuzz` without panic (issue #129); deleted entries, ADS, resident data readable. Ask the maintainer about a release.
  - Out of scope: path building, USN, wrapping it as `mft-parse`.
  - Fallback: pin git commit → fork → port from `ntfs-core`.
- [x] **S3 `ntfs-reader`** (Windows, admin) → ADR 0005
  - Done when: on a GitHub-hosted Windows runner (admin, disposable): raw `$MFT`; `$UsnJrnl:$J` without the sparse region; `$Secure:$SDS`, `$Boot`; no `unsafe` in our code; outputs uploaded as a workflow artifact. 4Kn recorded as a known limit.
  - Out of scope: zip packaging, `meta.json`, CLI, code signing, client Windows (runners are Windows Server).
  - Fallback: `std::fs::File` on `\\.\C:` + aligned reader + `ntfs-core` `NtfsFs`; or run the spike on the Windows VM.
  - Human steps: approve `ntfs-reader` (H2, agent vets it first); create a remote (private is fine), push the spike branch and trigger the `workflow_dispatch` job; download the artifact outside the repo.
- [x] **S4 VanillaWindowsReference → `fst`** (public images) → `docs/research/baseline-poc.md`
  - Done when, on public images matched to VWR builds (Windows 11: Magnet Virtual Summit 2023 `PC-MUS-001.E01`; Windows 10 22H2 (19045): a smaller public image or triage set): the image's build is identified; outside-baseline files created in the time window (7 days) measured as a share of all files (< 10 % target; `product.md` §4 metric); `$UpCase` MD5 recorded per build (ADR 0009 expects `7ff498a4…` for Win7 and later; if not, supersede it).
  - Out of scope: other Windows builds, baseline CI, distribution format; clean-install comparison and Win11 24H2 (moved to the Phase 1 gate).
  - Fallback: more normalization rules, or bring the baseline CI forward.
  - Human steps: `brew install sleuthkit` (reads E01 via libewf); download the images outside the repo (e.g. `~/evidence/public/`) and tell the agent the path. Never commit them; use them under their publishers' terms (training/research).

- [x] **ADR 0006**: GPL-3.0 data (LOLBAS, HijackLibs, winbindex) is AGPL-side data matched at runtime, never compiled into DRL rules.
  - Done when: ADR accepted and consistent with `.claude/rules/dependencies.md`.
  - Out of scope: importing the data.
- [x] **`ntfs-types`** (independent of spikes; one TDD cycle per bullet)
  - [x] `FileRef`: 48-bit entry + 16-bit sequence; `u64` round-trip
  - [x] `Filetime`: `u64` newtype (UTC, 100 ns since 1601); ordering; tests for 0, `u64::MAX`. Unix-time conversion deferred until a consumer needs it
  - [x] `NtfsName`: `Box<[u16]>`; escaped display; proptest round-trip of any `u16` sequence
  - [x] `NormPath`: built-in default `$UpCase`; case folding incl. non-ASCII
  - [x] Make the CI `coverage` job blocking (remove `continue-on-error`)
  - Done when: `cargo nextest run -p ntfs-types` and `cargo llvm-cov --workspace --fail-under-lines 90` pass.
  - Out of scope: serde, string parsing, time-zone conversion, path normalization beyond `$UpCase` (belongs to `baseline`).

Inputs needed: public Windows 11 and 10 images (S4); a GitHub remote for the runner (S3); a Win11 24H2 x64 VM before the Phase 1 gate.

**Windows VM (Phase 1 gate)**: Windows 11 24H2 **x64** (VanillaWindowsReference is x64; an ARM64 VM on Apple Silicon has a different file layout). Options: a spare x64 PC, a cloud Windows 11 VM, or UTM with x64 emulation (slow but enough). Install from the Microsoft Evaluation Center ISO, no extra software, record the build (`winver`, UBR). Collect `$MFT` and `$UpCase` right after install and after normal use, with the S3 spike `.exe`, FTK Imager or Velociraptor.

## Phase 1 (v0.1): walking skeleton

Goal: one end-to-end path from a `$MFT` file to a report, then widen it. Each slice is one branch; each slice keeps `mozu analyze` working end to end. Gate (H4): outside-baseline files in the window < 10 % of all files on a Win11 24H2 `$MFT` after normal use (Windows VM), and the `product.md` §4 target (< 5 %) revisited with that result; VWR gaps listed against the same VM right after a clean install; that VM's `$UpCase` MD5 checked (ADR 0009); 1 GB `$MFT` in ≤ 1 min and ≤ 2 GB RAM (measured on a synthetic 1 GB `$MFT` built with the test builder, on the Mac).

Decisions before the first slice (H2):
- [x] **Dependencies for Phase 1** ([ADR 0010](adr/0010-phase1-dependencies.md)): `thiserror` (libs), `clap` + `anyhow` (cli), `csv` (report), `fst` (baseline), `serde_json` + `serde` derive (report data). Vet each like ADR 0008; `cargo vet` exemptions or audits; `deny.toml` `allow-git` for `mft` (ADR 0004).
- [x] **ADR: which time is "created" for an MFT entry** ([ADR 0011](adr/0011-created-time.md)): `CreationUtcTime` from `$SI`, `FnCreationUtcTime` from `$FN`, window on either.
- [x] **ADR: normalization rules format** ([ADR 0012](adr/0012-path-normalization-rules.md)): Rust table in Phase 1 (S4 rules, NUL-prefixed placeholders), YAML in Phase 3.
- [x] **ADR: fuzzing in CI** ([ADR 0013](adr/0013-fuzzing-in-ci.md)): separate `fuzz/` workspace on pinned nightly, weekly CI; first target and NCSA in `deny.toml` come with P1-1.

- [x] **P1-1 `mft-parse` + skeleton CLI** (crates `mft-parse`, `analyze`, `report`, `cli`; [ADR 0014](adr/0014-mft-overflow-checks.md))
  - Done when: `mozu analyze -i <$MFT> --csv out.csv` writes one row per FILE record (entry, sequence, in-use, `$FN` names via `NtfsName`, `$SI`/`$FN` created times); corrupt records become `Diagnostic` rows, never a panic; names built only from `as_utf16le_bytes()` (ADR 0004); test builder moved from `spikes/s2-mft` into `mft-parse` tests; proptest no-panic properties; a `cargo fuzz` target.
  - Out of scope: paths, baseline, Sigma, HTML, non-resident data, `$ATTRIBUTE_LIST`.
- [x] **P1-2 Paths from `$MFT`** (crate `resolve`, MFT-only)
  - Done when: each entry gets a full path from parent references with sequence checks; state `resolved` / `unknown` (parent missing, deleted or reused); never invents a path; CSV gains a `path` column.
  - Out of scope: USN, Rewind, `inferred` (Phase 2).
- [x] **P1-3 Baseline** (crate `baseline`, `mozu baseline build`)
  - Done when: `mozu baseline build --vwr <csv> -o win11-24h2.fst` builds an fst of `NormPath` keys from VanillaWindowsReference Win11 24H2; normalization covers user profile, SIDs, GUIDs, WinSxS version parts; lookup gives `standard` / `outside`; CSV gains a `baseline` column; S4's outside-baseline ratio reproduced by the real code.
  - Out of scope: other builds, UBR fallback, zstd packaging, baseline CI, `mozu baseline update`.
- [x] **P1-3b Merge extension records** (`mft-parse`; added after P1-3: 4,982 duplicate rows on Win11)
  - Done when: `Entry.base` from the FILE header; `merge_extensions` appends an extension's names and diagnostics to its base (base readable and a base record, same sequence, same in-use state) and drops its row; any other extension keeps its row with `orphan_extension`; no duplicate in-use paths on the Win11 image; Win11 22H2 re-measured at 214,385 files / 69.1 % outside (S4's 70.7 % ignored extension names), Win10 unchanged (63,350 / 82.5 %).
  - Out of scope: `$ATTRIBUTE_LIST` parsing, non-resident attributes, `$SI` from extensions.
- [x] **P1-4 Sigma** (crates `sigma`, `detect`)
  - Done when: rules from a directory load through `rsigma-eval` (ADR 0003 rules: every rule has an `id`; route with `evaluate_with_logsource`); MFT entries emitted as `file_event` with standard fields, plus `service: baseline_outside` for outside entries; findings carry rule `id`, `title`, `level`, `author` (DRL) and the `explain` trace; results sorted (ADR 0002 #3); 3–5 sample rules in `testdata/rules/`.
  - Out of scope: correlations, USN categories (`file_delete` etc.), SigmaHQ import at scale, `mozu rules update`.
- [x] **P1-5 Minimal HTML report** (crate `report`)
  - Done when: `-o report.html` writes one self-contained file: host/input summary, findings table (level → time), outside-baseline list; data embedded Base64, no `innerHTML`; CSV formula neutralization; AGPL footer with source URL and commit; rule `author` shown (ADR 0001); attacker-name tests (`</script>`, `=cmd|…`, control chars, unpaired surrogates).
  - Out of scope: Svelte viewer, timeline, filters, en/ja.
- [ ] **P1-6 Gate measurements**
  - Done when: synthetic 1 GB `$MFT` (≈1M records) generator in `xtask` or a test helper; `mozu analyze` on it ≤ 1 min and ≤ 2 GB peak RSS on the Mac; determinism test (two runs, byte-identical outputs) in CI; outside-baseline ratio, VWR gaps and `$UpCase` MD5 on the Windows VM recorded in `docs/research/baseline-poc.md`.
  - Out of scope: optimization beyond the gate.
- [x] **P1-7 JSONL timeline** (`report`, `cli`)
  - Done when: `mozu analyze -i <$MFT> --jsonl out.jsonl` writes one JSON object per record, one per line, with the CSV columns as typed fields (numbers, bools, `null` when empty, `findings` as `[{level, id}]`, `diagnostics` as an array); names, paths and rule ids are display text (ADR 0002 #4) but not formula-neutralized; `--jsonl` combines with `--csv`/`-o` in one run and counts as the required output; it refuses to overwrite the input or another output, is replaced atomically, and is byte-identical across runs.
  - Out of scope: findings-only JSONL and Hayabusa/Takajo field names (M9), single-document JSON, stdout (`-`), renaming `-o`.
- [x] **P1-8 Sample rule: WinSxS is not "outside System32"** (`testdata/rules`; added after a 9-host Win7 run: 6 high false positives per host on `\Windows\winsxs\…\smss.exe` / `lsass.exe`)
  - Done when: `system_binary_outside_system32.yml` also excludes `C:\Windows\WinSxS\` (any case); an end-to-end test shows a `\Windows\winsxs\<component>\lsass.exe` gets no finding while `\Windows\svchost.exe` still does; rerun on the 9 local hosts gives 0 WinSxS findings (counts recorded in the hand-over, not in the repo).
  - Out of scope: other sample rules, a WinSxS component-name check, SigmaHQ rules, the HTML ordering of findings.
- [x] **P1-9 Sample rules for attack artifacts** (`testdata/rules`; [ADR 0016](adr/0016-rule-levels-by-false-positive-tolerance.md); added after a Win11 24H2 case where the sample rules found nothing)
  - Done when: five rules with levels per ADR 0016: SharpHound/BloodHound output (high), remote-access-tool files (medium), executables directly in `\Users` (medium), scripts and archives in `\Users\Public` (medium), executables in Temp (low); one e2e test per rule with a near-miss that does not match; no existing test expectation changes; a run on the 10 local hosts reports per-rule, per-level counts in the hand-over (not in the repo), with all five paths from that case flagged.
  - Out of scope: SigmaHQ import, the `tool-rules` repo, a ProgramData-root rule, baseline-routed variants, HTML filters, `explain` texts.
- [x] **P1-10 Remote-access-tool rules from LOLRMM** (`xtask`, `sigma`, `testdata/rules`; [ADR 0017](adr/0017-lolrmm-derived-rules.md), [ADR 0018](adr/0018-lolrmm-generation-details.md)) — superseded by P1-11 ([ADR 0019](adr/0019-generic-rules-only.md))
  - Done when: `cargo run -p xtask -- lolrmm <rmm_tools.json> <dir>` writes one rule per tool as ADR 0017 and ADR 0018 specify (normalization, deterministic ids, license header), byte-identical across runs; unit tests cover each normalization case (placeholder, `(x86)` variant, bare name, dropped catch-all) on a small inline JSON; the generated rules from commit `dc6ebe934bce` are committed with `LICENSE-LOLRMM` and a `NOTICE` entry; every generated rule loads; an e2e test shows AnyDesk, TeamViewer, Splashtop and ScreenConnect install paths (both `Program Files` variants) get their LOLRMM finding; `mozu analyze` on the 10 local hosts stays under 1 s each, with per-tool counts in the hand-over.
  - Out of scope: importing LOLRMM's Sigma rules, network/registry/process artifacts, macOS/Linux paths, fetching the JSON from the tool, RAT-specific levels, the `tool-rules` repo.
- [x] **P1-11 Remove product-specific rules and LOLRMM data** (`testdata/rules`, `xtask`, `NOTICE`; [ADR 0019](adr/0019-generic-rules-only.md); the maintainer found the product-named rules too narrow)
  - Done when: `testdata/rules/` holds only the 7 generic rules and no license file; the LOLRMM rules, `LICENSE-LOLRMM`, the `NOTICE` entry, `xtask lolrmm` and its `serde_json` dependency, the BloodHound and remote-access-tool rules and their tests are gone; `sigma` batch loading stays; no other test expectation changes; the full verify set passes.
  - Out of scope: rewriting git history, new generic rules, the `tool-rules` repo, ADR 0016.

## Phase 2 (v0.2): USN, facts, viewer

Goal: add `$J` to the same end-to-end path: USN events with their paths at the time of each event, a few generic rules that need USN, facts next to findings, a time window, and a viewer. Rules start as a small set of our own generic rules (ADR 0016, ADR 0019). External rule sets (SigmaHQ, hayabusa-rules) are not shipped, but a user can load them with `--rules`. Each slice keeps `mozu analyze` working end to end. Gate (H4): a non-expert decides the next step from a report; Rewind resolution ≥ usnjrnl_rewind.

- [x] **ADR: USN parsing and Rewind are our own code** ([ADR 0021](adr/0021-own-usn-parsing-and-rewind.md))
  - Proposal: write `usn-parse` and Rewind from Microsoft's `USN_RECORD_V2/V3/V4` documentation and the published Rewind algorithm (CyberCX, 2024). Use `ntfs-core` and usnjrnl_rewind only as test oracles, run outside the repo. No third-party code is copied, so `NOTICE` gains nothing. This supersedes "ported from `ntfs-core`" in `oss-reuse.md` and `architecture.md`.
  - Done when: ADR accepted; `oss-reuse.md` and `architecture.md` updated to match.
  - Out of scope: the code.
- [x] **P2-1 `usn-parse`** (new crate; `ntfs-types` only)
  - Done when: `$J` bytes → `UsnEvent` (USN, `FileRef` of the file and parent, `Filetime`, reason flags, attributes, `NtfsName`) for V2 and V3. V4 range records and unknown versions become a `Diagnostic` and are skipped. The leading zero (sparse) region is skipped without diagnostics. A truncated or corrupt record becomes a `Diagnostic`, never a panic, and parsing resumes at the next 8-byte-aligned record. Synthetic records come from a test builder. There are proptest no-panic properties and a `cargo fuzz` target. On the 9 Yamato hosts and Simulated-Case-1, the record count equals the oracle's (counts in the hand-over).
  - Out of scope: carving from free space, `$LogFile`, paths, the CLI.
- [x] **P2-2 `--usn <$J>` in `mozu analyze`** (`cli`, `analyze`, `report`)
  - Done when: `--usn` adds one row per USN event to CSV and JSONL (`source` = `mft`/`usn`, the USN, reason names, event time). Its path comes from the current `$MFT` with a sequence check, otherwise `unknown`. Rows are sorted by explicit keys, and output is byte-identical across runs. `--usn` without a readable `$J` fails with a message. Existing MFT rows are unchanged.
  - Out of scope: Rewind, detection on USN, directory/zip input (Phase 3, M2), HTML changes.
- [x] **P2-3 Rewind** (`resolve`)
  - Done when: each USN event gets the path it had at that moment, including renamed and deleted files and reused MFT entries. Its state is `resolved`, `inferred` (built partly from the journal) or `unknown`, and a path is never invented. Unit tests cover rename, delete, entry reuse and parent move on synthetic journals. On the 9 Yamato hosts and Simulated-Case-1, the share of events with a full path is ≥ usnjrnl_rewind's. Recorded in `docs/research/rewind.md`; this is the gate metric.
  - Out of scope: carving, cross-volume moves, `$LogFile`.
- [x] **P2-4 Sigma on USN events** (`detect`, `analyze`)
  - Done when: USN events are emitted as `file_event` (create), `file_delete`, `file_rename` (`SourceFilename`, `TargetFilename`) and `file_change`, with the USN time. Outside-baseline USN events also match `service: baseline_outside`. Findings carry the USN and the path state. An e2e test covers each category with a near-miss.
  - Out of scope: correlations (Phase 3), new rules.
- [x] **P2-5 Own generic USN rules** (`testdata/rules`; ADR 0016, ADR 0019)
  - Done when: 3–5 rules that use only generic facts (no product or tool names). The list and levels are approved at H1; candidates are `.evtx` deleted, a file renamed to an executable extension, and an executable deleted from Temp or Public. Each has one e2e test with a near-miss. A run on the 10 local hosts reports per-rule, per-level counts in the hand-over.
  - Out of scope: external rules, the `tool-rules` repo, correlations.
- [x] **P2-6 Load external rule sets** (`sigma`, `cli`)
  - Done when: `--rules` may be repeated. A rule the engine cannot use (correlation, unknown modifier or field, a logsource we never emit, a missing `id`) is skipped with a diagnostic naming the file and the reason. Skipped rules are counted in the HTML summary and listed in a CSV/JSONL side output, never silently dropped. A duplicate `id` across directories is still an error. Tests use small rules written for the test, in the shape of SigmaHQ rules; no third-party rule text is copied. Changes the expectation of `a_rule_without_id_fails_with_a_message_not_a_panic` (H5, approved with this item).
  - Out of scope: shipping or fetching external rules, `mozu rules update`, `explain` texts.
- [x] **P2-7 Facts** (`analyze`, `report`)
  - Done when: each row carries facts, shown next to findings and never used to order them. The facts are: deleted, seen in MFT/USN/both, ADS names (incl. `Zone.Identifier`), and timestomp hints (`$SI` created < `$FN` created; zero sub-seconds). They appear as CSV and JSONL columns and in HTML finding rows, with an e2e test per fact.
  - Out of scope: owner SID (`$SDS`, Phase 3), `Zone.Identifier` content, new rules on facts.
- [x] **P2-8 Time window** (`cli`, `analyze`, `report`)
  - Done when: `--from`, `--to` and `--tz` select a window on either created time (ADR 0011) or the USN time. Detection still runs on everything; the HTML lists only in-window rows, and CSV/JSONL gain `in_window`. The report warns when the window starts before the first USN event. Bad dates fail with a message. Output is byte-identical across runs.
  - Out of scope: per-screen filters in the viewer (P2-9b).
- [x] **ADR: viewer build** ([ADR 0020](adr/0020-viewer-build.md): plain JS on the current template, DOM virtual scroll; research in [research/viewer.md](research/viewer.md))
  - Done when: ADR accepted, including its dependency and supply-chain controls (H2).
  - Out of scope: the viewer code.
- [x] **P2-9a Folder category** (`analyze`, `report`)
  - Done when: each `$MFT` and USN row carries a `folder` category, written to CSV/JSONL (last column) and to HTML finding and outside-baseline items. It is the first match, comparing whole path segments case-insensitively (ASCII), in this order:

    | Folder | Path |
    |---|---|
    | Public | under `\Users\Public\` |
    | Downloads, Desktop, Documents | under `\Users\<name>\<that folder>\` |
    | Temp | any segment named `Temp` (`\Windows\Temp`, `AppData\Local\Temp`, `\Temp`) |
    | AppData | under `\Users\<name>\AppData\` |
    | User profile | elsewhere under `\Users\<name>\` |
    | ProgramData | under `\ProgramData\` |
    | Program Files | under `\Program Files\` or `\Program Files (x86)\` |
    | Windows | under `\Windows\` |
    | Other | any other known path |
    | no path | path unknown |

    Unit tests cover each category and the order (Public before the per-user folders, Temp before AppData and Windows).
  - Out of scope: the viewer, the privilege grouping (product.md §6).
- [x] **ADR: JS tests for the viewer** ([ADR 0022](adr/0022-viewer-js-tests.md))
  - Proposal: run the viewer's JS logic (filters, time range, URL hash, row mapping of the capped virtual scroll) as tests under Node with a small DOM stub, from `cargo nextest` or a separate CI step. Node becomes a CI tool, so this is an H2 decision; no npm packages.
  - Done when: ADR accepted. The CI job and the first tests come with P2-9b.
  - Out of scope: the viewer code.
- [x] **P2-9b Viewer: Summary, Findings, Outside-baseline** (`report`; per ADR 0020 and the JS-test ADR)
  - Done when: one self-contained HTML file with three tabs; the tab and filter state are kept in the URL hash. All times are UTC.

    | Screen | Content |
    |---|---|
    | Summary | Host, input, USN coverage (first .. last USN time), window and its warning, findings by level, outside-baseline count in the window, baseline and rule counts, record counts. The critical and high counts open Findings filtered to that level |
    | Findings | Columns level (as text), time, path, folder, rule; the facts on a small line under each row (`deleted` and ADS first, then `seen` and hints); "Why it matched" opens on demand. Fixed order level → time. Filters: level, rule, folder, time range (from/to, `to` inclusive) on `$SI` created, `$FN` created or the USN time, and a path substring search. Shows "N of M" |
    | Outside-baseline | Counts per folder on one line (click to filter); columns `$SI` created, path, MFT entry; filters folder, time range and path search |

    The payload is decoded with `Uint8Array.fromBase64` and a plain-loop fallback. Tables use the DOM virtual scroll of ADR 0020 (fixed row height, capped spacer). JS tests cover each filter, the inclusive `to`, URL hash round trip and the scroll row mapping. All P1-5 attacker-name tests still pass, there is no `innerHTML`, and output is byte-identical across runs.
  - Out of scope: sorting by other columns, filters on facts, grouped tables, a time zone switch, Timeline and File detail screens, en/ja (Phase 3), the privilege grouping (product.md §6).
- [x] **P2-9c Viewer performance** (`report`)
  - Done when: with 1 M synthetic rows, a filter or search finishes in ≤ 200 ms (measured by the agent under Node), and scrolling to the last row shows it in Chrome and Firefox (checked by the human). Results recorded in [research/viewer.md](research/viewer.md).
  - Out of scope: gzip and the other escalation steps of ADR 0020 unless the measurement requires them (each by its own ADR).
- [x] **P2-9d Outside-baseline binary rule: skip transient Windows folders** (`testdata/rules`, ADR 0016 revised; added after a Simulated-Case-1 run: with the matching Server 2025 baseline, 164–337 high findings per host, mostly temp, installer and servicing files)
  - Done when: `outside_baseline_windows_binary.yml` excludes `C:\Windows\Temp\`, `SystemTemp\`, `Installer\` and `WinSxS\Temp\`; an e2e test shows a file in each gets no finding while `System32`, `WinSxS` components and `\Windows\Temp2\` still do; ADR 0016 defines `high` as reviewable rather than near-zero false positives; high counts on the 3 Simulated-Case-1 hosts recorded in the hand-over.
  - Out of scope: excluding the VC++ runtime or `assembly`, baseline selection by edition (Phase 3), other rules.
- [x] **P2-9e Baseline edition check** (`baseline`, `analyze`, `report`, `cli`; added after Simulated-Case-1, all Windows Server 2025, was analyzed with the Win11 baseline: about 3,000 high findings per host)
  - Done when: the HTML Summary shows an `Edition` row: the names of the in-use directories `\Windows\System32\spp\tokens\skus\<name>` (ASCII case-insensitive), sorted and comma-separated; with a baseline in which none of those names has a file under its folder, a `Baseline warning` row names the edition and the baseline and asks for a baseline of the same edition. New `pub`: `Baseline::has_folder`, `analyze::sku_folder`, `HtmlReport::check_edition`. CSV/JSONL unchanged; output byte-identical across runs. On real data (hand-over): Simulated-Case-1 with the Win11 baseline warns, with the Server 2025 baseline does not; Yamato without a baseline shows only the edition.
  - Out of scope: choosing the baseline automatically, storing the edition in the baseline file, build/UBR checks, CSV/JSONL columns, registry parsing.
- [x] **P2-P Prune the report and viewer** (`report`, `cli`; [research/viewer-review.md](research/viewer-review.md); the phase's prune item, AGENTS.md)
  - Done when: every Summary row, viewer control and HTML data field is listed in the hand-over with the `product.md` need, plan item or test that justifies it; those without one are removed after human approval at H1 (H5 for each test expectation that changes). Lines of `viewer.js` and `html.rs` and the desktop6 HTML size before/after are in the hand-over. CSV/JSONL unchanged; output byte-identical across runs.
  - Out of scope: adding or changing behavior, CSV/JSONL columns, the other fixes in research/viewer-review.md.
- [x] **P2-11 Viewer defects** (`report`; [research/viewer-review.md](research/viewer-review.md) U5, U6, U7, part of U8)
  - Done when: a filter or search change replaces the current history entry (`history.replaceState`) and only a tab change adds one; the rule drop-down is ordered by level, then title; a cut rule title shows in full as a tooltip; the clicked row stays highlighted while the list scrolls or repaints and the detail pane is scrolled into view; the from/to inputs show their whole placeholder. One JS test per behavior (the placeholder is checked by the human); Back and the hash work from a `file://` report in Chrome and Firefox (checked by the human). CSV/JSONL unchanged; output byte-identical across runs.
  - Out of scope: layout changes (e.g. a side pane), keyboard navigation, new data, the other fixes in research/viewer-review.md.
- [x] **P2-12 Viewer: file state column** (`report`; added after a Simulated-Case-1 review: the intended user knows a suspicious file but not the MFT layout, and needs to see whether a file is deleted)
  - Done when: Findings columns are level, state, time, path, folder, rule, and Outside-baseline columns are `$SI` created, state, path (no MFT entry column; the entry stays in the detail pane). State is `deleted` or `present`; `deleted` is no longer in the facts line, and the search matches the state. The Outside-baseline data carries `deleted`. CSV/JSONL unchanged; output byte-identical across runs.
  - Out of scope: a state filter control, other columns, CSV/JSONL columns.
- [x] **P2-13 Viewer: state filter** (`report`; added after a Simulated-Case-1 review: the single search box cannot combine a path with "deleted only")
  - Done when: Findings and Outside-baseline show `State:` chips `present N` / `deleted N` that filter the list like the folder chips and combine with the other filters; the state is kept in the URL hash (`state=`); the search no longer matches the state (one way to filter it). CSV/JSONL unchanged; output byte-identical across runs.
  - Out of scope: other new filters, Summary links to the state filter, CSV/JSONL columns.
- [x] **P2-14 Timestomp rule** (`detect`, `analyze`, `testdata/rules`; [research/detection-gaps.md](research/detection-gaps.md) T1070.006; `$SI` < `$FN` holds for 76–90% of files on real hosts, so the fact alone is no rule)
  - Done when: `$MFT` `file_event`s carry `SiCreatedBeforeFnCreated` (`true`/`false`) when both times are known, listed in `detect::EMITTED`; the `si_before_fn` hint uses the same comparison (new `pub fn detect::si_before_fn`). One `medium` rule matches outside-baseline executables (the extensions of `renamed_to_executable.yml`) with the field `true`, outside `C:\Program Files\` and `C:\Program Files (x86)\`, with one e2e test and near-misses. Per-host counts on Simulated-Case-1 in the hand-over. CSV/JSONL/HTML columns unchanged; output byte-identical across runs.
  - Out of scope: zero sub-seconds as a field, USN events, excluding the VC++ runtime, other rules, the upstream items of detection-gaps.md.
- [x] **P2-15 Alternate data stream rule** (`detect`, `analyze`, `testdata/rules`; [research/detection-gaps.md](research/detection-gaps.md) T1564.004; outside `Zone.Identifier` and known names, 12 real hosts hold only NTFS system streams and one DFSR stream)
  - Done when: each named `$DATA` stream of a `$MFT` file is a `create_stream_hash` event (Sysmon 15 shape) with `TargetFilename` = `C:\<path>:<stream>`, and `create_stream_hash` is listed in `detect::EMITTED` (new `pub fn detect::stream_events`); a row's findings list each rule once. One rule matches any stream except `Zone.Identifier`, the default exclusions of Velociraptor's `Windows.NTFS.ADSHunter` and files under `C:\$`; level `medium` (H1). One e2e test with near-misses (`Zone.Identifier`, an excluded name, `\$Secure:$SDS`). Per-rule counts on the 12 local hosts in the hand-over. CSV/JSONL/HTML columns unchanged; output byte-identical across runs.
  - Out of scope: stream content, `Contents`/`Hash` fields, USN stream events (`NAMED_DATA_*`), directories' streams, other rules.
- [x] **P2-16 Timestomp rule: skip the Installer cache** (`testdata/rules`; P2-14 hand-over: 4–10 `.msi` per Simulated-Case-1 host in `\Windows\Installer\`, which Windows Installer stores with the package's times)
  - Done when: the timestomp rule excludes `C:\Windows\Installer\` (the folder P2-9d excludes); an e2e test shows `\Windows\Installer\1a2b.msi` and `\Windows\Installer\{GUID}\x.exe` with `$SI` < `$FN` get no finding while `\Windows\Installer2\x.msi` does. Per-host counts on Simulated-Case-1 before and after in the hand-over. No other test expectation changes; output byte-identical across runs.
  - Out of scope: excluding the VC++ runtime, dropping `.msi` from the extensions, other rules.
- [x] **P2-17 Viewer: one-line filter bar and full-height list** (`report`; added after a Simulated-Case-1 review: the filters take 5–6 lines and the list is fixed at 60vh, so about 14 rows show at 1080p; [research/viewer-review.md](research/viewer-review.md) U6)
  - Done when: Level, Folder and State are drop-downs like Rule, each option `<value> (<n>)` for the values present plus `all …`; a value from the hash that the report lacks stays visible in its drop-down, as Rule does now. Search, From, To, Level, Rule, Folder, State, the `N of M` count and Clear sit in one bar that wraps on a narrow window. The page fills the window: the list takes the height left, the detail pane has a capped height with its own scroll, the title and the UTC note share one line, and a window resize repaints the list. Rows visible at 1920×1080 in Chrome and Firefox before/after (checked by the human), `viewer.js` lines and desktop6 HTML size before/after in the hand-over. URL hash keys unchanged; CSV/JSONL unchanged; output byte-identical across runs.
  - Out of scope: one-line rows, a side pane, keyboard navigation, column or Summary changes, CSV/JSONL.
- [ ] **P2-10 Gate material**
  - Done when: the Rewind comparison (P2-3) is summarized, and a report from Simulated-Case-1 is ready for a non-expert session. The human runs the session and ticks the gate.
  - Out of scope: optimization beyond the gate.

## Risks

| Risk | Fallback |
|---|---|
| `mft` fix never released | Pin git commit → fork → port from `ntfs-core` |
| `rsigma-eval` API churn | Pin `=0.x.y` behind the adapter; switch to Hayabusa's engine if costly |
| `ntfs-reader` cannot skip the `$J` sparse region | `ntfs-core` `NtfsFs` approach |
| VanillaWindowsReference lacks hidden/system files | Supplement with one clean-VM `$MFT` in Phase 1 |
| Drift from ported `ntfs-core` code | Record source commit in `NOTICE`; review upstream periodically |
