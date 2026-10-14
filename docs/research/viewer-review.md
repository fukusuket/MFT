# Viewer review and fix plan

Status: **draft** (not yet in [plan.md](../plan.md); items move there after H1 approval).
Source: UX and threat-hunting/DFIR review of the HTML report at `e681e8a` (Simulated-Case-1 desktop6, Server 2025 baseline, `testdata/rules`).
Evidence-derived names are left out on purpose ([security.md](../security.md)).

## 1. Findings

### UX defects

| # | Problem | Evidence |
|---|---|---|
| U1 | The same file is listed twice, once from `$MFT` and once from USN | high: 62 rows, 34 paths; all levels: 159 rows, 123 paths |
| U2 | The time column mixes `$SI` created and USN event time without saying which | `viewer.js`: `f.created ?? f.event_time`; USN reason not shown |
| U3 | The detail pane shows only the raw `explain` JSON | No rule `description`/`falsepositives` in the report data; no Why/Limits/Check (product.md §6) |
| U4 | A rename finding does not show the old name | `SourceFilename` is used for detection but not passed to the HTML |
| U5 | Every keystroke in a filter adds a browser history entry | `go()` sets `window.location.hash`; Back stops working |
| U6 | The detail pane sits below the 60vh list; on a laptop a click shows nothing; no selected-row highlight | Screenshot at 1400×1100 |
| U7 | The rule drop-down is sorted by rule UUID | `filters()` sorts on `a[0]` (the id) |
| U8 | Minor: rule cell has no tooltip; date placeholder cut off; facts show internal tokens (`si_before_fn`) and `seen: both` on almost every row; 7-digit fractions and no thousands separators; header does not say "newest first"; `Host: not available ($MFT only)` reads as an error | Screenshots |

### Threat-hunting / DFIR gaps

| # | Gap | Evidence |
|---|---|---|
| H1 | Reading level → time does not reach the attack | 25 of 34 high paths are VC++ runtime DLLs; the attack files are medium/low next to 34 PowerShell-policy-test and diagnostics scripts |
| H2 | No per-directory counts of outside-baseline files | 14,321 flat rows; three remote-access tools (300–500 files each) only show up when counted by directory |
| H3 | No timeline or time pivot (± minutes, same parent, same MFT entry) | An install chain over ~2 minutes is spread over three screens |
| H4 | Only created times; no M/A/C/E | Changes to existing files are invisible |
| H5 | No file size | Size is in `$MFT` |
| H6 | No execution trace | Prefetch `.pf` creation is the closest NTFS-only signal (disabled by default on Server) |
| H7 | No "who" | Until owner SID (Phase 3), the user name in the path is the cheapest proxy |
| H8 | Outside-baseline rows carry no facts and are ordered by `$SI` created only | deleted/seen/hints/`$FN` missing |
| H9 | HTML rows cannot be mapped back to CSV/JSONL rows | Entry sequence and USN not shown in the list |

## 2. Plan

Two stages. Stage A goes before the P2-10 gate session so a non-expert is not stopped by noise or raw JSON. Stage B makes the Phase 3 items Timeline and File detail concrete.

Principles ([AGENTS.md](../../AGENTS.md#simplicity-unix-philosophy)): prune before adding; each item names what it removes; anything an expert can do with CSV/JSONL and `jq`/`sort` stays out of the viewer unless the primary user needs it to decide the next step.

### Stage A: before P2-10 (Phase 2)

Order: P2-P → P2-11 → ADR (finding is a file) → P2-12 → P2-13 → P2-14 → P2-15. P2-11 and P2-15 have no dependencies on the others.

- [ ] **P2-P Prune the report and viewer** (`report`, `cli`; before anything is added)
  - Done when: every Summary row, viewer control and report data field is listed with the `product.md` need or test that justifies it; those without one are removed after human approval (H5 for each test that changes). Candidates to check: the "(not in this report)" rule option for shared links; Summary rows that only restate CSV/stderr diagnostics (`Record diagnostics`, `MFT entries / files in use`, the `none` level); data fields the viewer never reads. Lines of `viewer.js`, `html.rs` and the HTML size before/after in the hand-over.
  - Out of scope: adding anything, CSV/JSONL columns.
- [ ] **P2-11 Viewer defects** (`report`; U5, U6, U7, part of U8)
  - Done when: filter and search changes use `history.replaceState` (only a tab change adds a history entry); the rule drop-down is ordered by level, then title; the rule cell has a tooltip; the selected row is highlighted and the detail pane is scrolled into view; the date placeholders fit. One JS test per behavior; history is checked with a `history` stub.
  - Removes: hash writes on every keystroke.
  - Out of scope: layout changes, keyboard navigation, new data.
- [ ] **ADR: a finding is a file** (H2 decision)
  - Proposal: one list item per file (MFT entry + sequence) with all its matched rules and USN events. Its level is the highest matched level, and its time is the file's first event. Order stays level → time. Rows without an entry or path stay one per event. Summary counts become file counts. Nothing is scored.
  - Done when: ADR accepted by a human.
  - Out of scope: the code.
- [ ] **P2-12 One row per file in Findings** (`report`; U1, U2)
  - Done when: per the ADR, a file's `$MFT` row and USN rows form one item; a reused entry (other sequence) is a separate item; the time cell names its source (`$SI created`, `$FN created` or `USN <reason>`); desktop6 high rows drop from 62 to 34 (hand-over); CSV/JSONL unchanged; output byte-identical across runs.
  - Removes: the duplicate per-event rows (U1).
  - H5: changes Summary count expectations; approved with this item.
  - Out of scope: File detail screen, Outside-baseline screen.
- [ ] **P2-13 Rename source and USN reasons in the HTML** (`analyze`, `report`; U4, U2)
  - Done when: a rename finding carries `SourceFilename` (the old path) into the HTML data and detail pane; USN rows show their reason names; one e2e test each.
  - Out of scope: new CSV/JSONL columns.
- [ ] **P2-14 Rule explanations** (`sigma`, `report`, `testdata/rules`; U3)
  - Done when: `sigma::Finding` gains `description` and `falsepositives` (new `pub` fields), shown in the detail pane; the `explain` trace is rendered as one line per field check (field, matcher, pattern, actual value, ✓/✗); the 11 own rules have a `description` (why it matters, what to check) and `falsepositives`.
  - Removes: the raw JSON in the detail pane (the lines carry the same data).
  - Out of scope: en/ja (Phase 3), changes to rule conditions or levels.
- [ ] **P2-15 Plain facts and numbers** (`report`; U8)
  - Done when: hint tokens become sentences (e.g. `si_before_fn` → "$SI created is earlier than $FN created (possible timestomping)"); `seen` is shown only when it is MFT-only or USN-only; list times end at seconds (full precision in detail); numbers have thousands separators; the host row explains that `$MFT` has no host name; the header says "newest first".
  - Removes: `seen: both` on every row; internal hint tokens.
  - Out of scope: new Summary rows, colors.

### Stage B: Phase 3

Trimmed by the principles above: no activity sparkline, no "same parent directory" pivot (the search does it), no "executables only" toggle (search `.exe`), no user column (search the user name). The phase ends with a prune item (P3-P).

- [ ] **P3-a Outside-baseline by directory** (`report`; H2)
  - Done when: the top-most directory that is not in the baseline is listed with file count, first created time and executable count; clicking it filters to its files; order is explicit (first created, then path); on desktop6 the three remote-access tool directories are on the first screen (hand-over).
  - Out of scope: product names, any ranking by count. This only counts files; it is not detection.
- [ ] **P3-b File detail** (`mft-parse`, `analyze`, `report`; U3, H4, H5, H8, H9)
  - Done when: all `$SI`/`$FN` times (M/A/C/E), size, entry and sequence, ADS, hints and the file's USN lifecycle (create → rename → delete) are shown; it opens from Findings and Outside-baseline rows.
  - Out of scope: resident data content, `$SDS` owner.
- [ ] **P3-c Timeline and pivots** (`report`; H3)
  - Done when: all in-window events of findings and outside-baseline files are listed by time; any row offers "± 5 min" (the same file's events are in File detail); the pivot is kept in the URL hash.
  - Out of scope: a time zone switch.
- [ ] **P3-d Summary guidance** (`report`)
  - Done when: key events (critical/high files, deleted `.evtx`), "what this report cannot tell you" (including Prefetch being off by default on Server) and baseline metadata (build/UBR, created date, source) are shown.
  - Out of scope: scores or rankings.
- [ ] **P3-e Facts on outside-baseline rows** (`report`; H8)
  - Done when: rows show deleted, MFT-only/USN-only, hints and `$FN` created.
  - Out of scope: new rules, extension filters.
- [ ] **P3-P Prune before the Phase 3 gate** (all crates)
  - Done when: as P2-P, over the whole workspace (features, `pub` items, CLI flags, report rows); removals approved by a human.
  - Out of scope: adding anything.

### Rules and baseline (no viewer code)

- [ ] **R-1 Baseline build and normalization** (ADR, H2; H1)
  - Proposal: build baselines from installed **and updated** images (Windows Update, Defender, Edge, a few days of use); normalize versioned and GUID directories (WindowsApps package versions, Edge and Defender platform versions), extending ADR 0012. Judge the VC++ runtime false positives by whether the baseline removes them, not by a rule exclusion.
  - Done when: ADR accepted; outside-baseline share in the window on Simulated-Case-1 re-measured against the < 5 % MVP target.
  - Out of scope: baseline CI (Phase 3 item).
- [ ] **R-2 Prefetch creation rule** (`testdata/rules`; H6)
  - Done when: one generic rule with an e2e test and a near-miss, as in P2-5.
  - Out of scope: parsing `.pf` content.

## 3. Open decisions

| Decision | Recommendation |
|---|---|
| How much of Stage A goes before the P2-10 session | All of it; at minimum P2-12 and P2-14 |
| Finding unit = file (ADR) | Accept; it changes Summary counts (H5) |
| H7 (who) | No user column; search the user name until `$SDS` owner (Phase 3) |
