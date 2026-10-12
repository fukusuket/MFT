# tool (working name)

Fast forensic triage of Windows NTFS using only `$MFT` and `$UsnJrnl:$J`. It subtracts a Windows baseline and focuses on a time window to surface the files and file activity worth investigating, even when event logs were wiped. Early development.

- Small collection (tens to hundreds of MB); one self-contained HTML report
- Detection by Sigma rules with fixed levels; no computed scores

See [docs/product.md](docs/product.md). Contributing: [AGENTS.md](AGENTS.md).

## Features
Phase 1 (walking skeleton) works on a raw `$MFT` only. Progress: [docs/plan.md](docs/plan.md).

| Feature | What it does |
|---|---|
| `$MFT` parsing | One row per FILE record: entry, sequence, in-use flag, `$FILE_NAME` names, `$SI` and `$FN` created times. Extension records are merged into their base record. A corrupt record becomes a diagnostic (`bad_signature`, `fixup_mismatch`, `truncated`, `malformed`, `orphan_extension`) and never crashes the tool. |
| Path resolution | Builds full paths by following parent references, with sequence checks. If a parent is missing, deleted or reused, the path is `unknown`; the tool never guesses one. |
| Windows baseline | `tool baseline build` turns a VanillaWindowsReference CSV into an `fst` file. User profiles, SIDs, GUIDs and WinSxS versions are normalized, so each path is marked `standard` or `outside`. |
| Sigma rules | Loads rules from each `--rules` directory (repeatable, subdirectories included). Rules that could never match here are skipped, counted in the HTML summary and listed in `<output>.skipped-rules.csv`/`.jsonl` beside the CSV/JSONL outputs (beside the HTML report when it is the only output). Each entry is checked as a `file_event` with `TargetFilename`, `CreationUtcTime` and `FnCreationUtcTime`. With `--usn`, USN events are also checked as `file_event`, `file_delete`, `file_rename` or `file_change` ([mapping](docs/architecture.md#baseline-and-sigma)). Outside-baseline entries and events also match `service: baseline_outside`. A finding lists the rule's id, title, level and author. Sample rules: [testdata/rules](testdata/rules). |
| Time window | `--from` / `--to` (`YYYY-MM-DD` or `YYYY-MM-DDTHH:MM:SS`, both ends included; a date covers the whole day) and `--tz` (`±HH:MM`, default UTC). Detection still runs on everything; the HTML report lists and counts only findings and outside-baseline files in the window and warns when the window starts before the first USN event. An entry is in the window if its `$SI` or `$FN` created time is, a USN event by its time ([details](docs/architecture.md#time-window)). |
| HTML report | One self-contained file with three tabs. Summary: input, USN coverage, window, findings per level (critical and high open the filtered findings), counts. Findings: ordered by level and then time, each row's facts beneath it, "why it matched" on click. Outside-baseline files: created time, path, MFT entry. Both lists: a search over every shown column, a time range, folder chips with counts (findings also level chips and a rule list), the active filters with a remove button each, and Clear. Long paths keep their file name; the full path is the tooltip. Filters live in the URL hash; long lists scroll virtually. All times UTC. |
| CSV | One row per record: `entry`, `sequence`, `in_use`, `name`, `path`, `path_state`, `baseline`, `findings`, `si_created`, `fn_created`, `diagnostics`, `source`, `usn`, `reasons`, `event_time`, then the facts: `deleted`, `seen` (`mft`, `usn` or `both`), `streams` (alternate data stream names, `;`-separated; a `;` in a name shows as `\;`) and `hints` (`si_before_fn`, `si_zero_subseconds`), then `in_window` (empty without a window) and `folder` (where the file sits: `public`, `downloads`, `desktop`, `documents`, `temp`, `appdata`, `user_profile`, `programdata`, `program_files`, `windows`, `other`, `no_path`). Values that a spreadsheet would run as formulas are neutralized. |
| JSONL | One JSON object per line and record, with the CSV columns as typed fields: numbers, booleans, `null` when empty, `findings` as `[{level, id}]` and `diagnostics` as an array. |
| Safe outputs | Refuses to write over an input file or to use one file for two outputs. Each output (CSV, HTML, JSONL, baseline) is written in full and then swapped into place, so a failed run keeps the previous file and leaves no partial one. |

Not implemented yet: `$UsnJrnl:$J`, `$Boot` / `$Secure:$SDS`.

## Usage
```sh
cargo build --release   # binary: target/release/tool

# Optional: build a baseline from a VanillaWindowsReference CSV
tool baseline build --vwr <vwr.csv> -o win11-24h2.fst

# Analyze a raw $MFT (at least one of -o / --csv / --jsonl)
tool analyze -i <$MFT> -o report.html --csv out.csv --jsonl out.jsonl \
  --baseline win11-24h2.fst --rules testdata/rules

# Focus the report on a time window (Japan time here; output times stay UTC)
tool analyze -i <$MFT> --usn <$J> -o report.html --from 2026-09-28 --to 2026-10-02 --tz +09:00
```

## License
- Code: [AGPL-3.0-only](LICENSE)
- Detection rules: [DRL 1.1](https://github.com/SigmaHQ/Detection-Rule-License)
