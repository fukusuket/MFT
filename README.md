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
| Sigma rules | Loads rules from a directory. Each entry is checked as a `file_event` with `TargetFilename`, `CreationUtcTime` and `FnCreationUtcTime`. Outside-baseline entries also match `service: baseline_outside`. A finding lists the rule's id, title, level and author. Sample rules: [testdata/rules](testdata/rules). |
| HTML report | One self-contained file: an input summary (record counts, diagnostics, findings per level), a findings table ordered by level and then time, and the list of outside-baseline files. |
| CSV | One row per record: `entry`, `sequence`, `in_use`, `name`, `path`, `path_state`, `baseline`, `findings`, `si_created`, `fn_created`, `diagnostics`. Values that a spreadsheet would run as formulas are neutralized. |
| JSONL | One JSON object per line and record, with the CSV columns as typed fields: numbers, booleans, `null` when empty, `findings` as `[{level, id}]` and `diagnostics` as an array. |
| Safe outputs | Refuses to write over an input file or to use one file for two outputs. Each output (CSV, HTML, JSONL, baseline) is written in full and then swapped into place, so a failed run keeps the previous file and leaves no partial one. |

Not implemented yet: `$UsnJrnl:$J`, the time window, `$Boot` / `$Secure:$SDS`.

## Usage
```sh
cargo build --release   # binary: target/release/tool

# Optional: build a baseline from a VanillaWindowsReference CSV
tool baseline build --vwr <vwr.csv> -o win11-24h2.fst

# Analyze a raw $MFT (at least one of -o / --csv / --jsonl)
tool analyze -i <$MFT> -o report.html --csv out.csv --jsonl out.jsonl \
  --baseline win11-24h2.fst --rules testdata/rules
```

## License
- Code: [AGPL-3.0-only](LICENSE)
- Detection rules: [DRL 1.1](https://github.com/SigmaHQ/Detection-Rule-License)
