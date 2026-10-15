# tool (working name)

Forensic triage of Windows NTFS from `$MFT` and `$UsnJrnl:$J`, for when event logs are gone. It marks files outside a Windows baseline, runs Sigma rules and writes one self-contained HTML report. Pre-alpha (Phase 2 of [plan](docs/plan.md)).

## What it does

- Parses `$MFT` and `$J` (V2/V3), rebuilds paths of deleted and renamed files (Rewind)
- Marks each path `standard` or `outside` a baseline built from VanillaWindowsReference
- Runs Sigma rules on MFT entries and USN events; findings are listed by level, never scored
- Focuses the report on a time window; detection still runs on everything
- Writes HTML (Summary, Findings, Outside-baseline), CSV and JSONL; same input → same bytes; no network

## Quick start

```sh
cargo build --release                      # binary: target/release/tool

tool baseline build --vwr <vwr.csv> -o win11-24h2.fst

tool analyze -i '$MFT' --usn '$J' \
  --baseline win11-24h2.fst --rules testdata/rules \
  --from 2026-09-28 --to 2026-10-02 --tz +09:00 \
  -o report.html --csv timeline.csv --jsonl timeline.jsonl
```

Open `report.html` in any browser. Rules that could not load are listed in `*.skipped-rules.csv` / `.jsonl`.

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
