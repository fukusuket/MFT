# mozu

Forensic triage of Windows NTFS from `$MFT` and `$UsnJrnl:$J`, for when event logs are gone. It marks files outside a Windows baseline, runs Sigma rules and writes one self-contained HTML report. Pre-alpha (Phase 2 of [plan](docs/plan.md)).

Current limits: input is a raw `$MFT` and an optional raw `$J`, not a collection directory or zip. USN V2/V3 are parsed; V4 is reported as a diagnostic. Correlation rules, ready-made baselines, collection, Timeline/File detail screens and localization are planned for Phase 3.

## What it does

- Parses `$MFT` and `$J` (V2/V3), rebuilds paths of deleted and renamed files (Rewind)
- Marks each path `standard` or `outside` a baseline built from VanillaWindowsReference
- Runs Sigma rules on MFT entries and USN events; findings are listed by level, never scored
- Focuses the report on a time window; detection still runs on everything
- Writes HTML (Summary, Findings, Outside-baseline), CSV and JSONL; same input → same bytes; no network

## Quick start

```sh
cargo build --release                      # requires Rust 1.98; binary: target/release/mozu

./target/release/mozu baseline build --vwr <vwr.csv> -o win11-24h2.fst

./target/release/mozu analyze -i '$MFT' --usn '$J' \
  --baseline win11-24h2.fst --rules testdata/rules \
  --from 2026-09-28 --to 2026-10-02 --tz +09:00 \
  -o report.html --csv timeline.csv --jsonl timeline.jsonl
```

The baseline CSV is a VanillaWindowsReference export from the same Windows edition as the evidence; it must contain a `FullName` column. Open `report.html` in any browser. Rules that could not load are listed in `*.skipped-rules.csv` / `.jsonl`.

## Docs

| | |
|---|---|
| [Use cases](docs/usecases.md) | Who uses it and how, step by step |
| [Product](docs/product.md) | Problems, users, MVP |
| [Architecture](docs/architecture.md) | Pipeline, crates, outputs |
| [Plan](docs/plan.md) | Current phase and next items |
| [AGENTS.md](AGENTS.md) | Development rules |

## License
- Code: [AGPL-3.0-only](LICENSE)
- Detection rules: [DRL 1.1](https://github.com/SigmaHQ/Detection-Rule-License)
