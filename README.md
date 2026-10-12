# tool (working name)

Fast forensic triage of Windows NTFS from `$MFT` and `$UsnJrnl:$J`. It compares paths with a Windows baseline, applies Sigma rules and writes a self-contained HTML report. Early development.

## Features

- Parses `$MFT` and `$UsnJrnl:$J`, including corrupt-record diagnostics and historical path reconstruction
- Marks paths as standard or outside a VanillaWindowsReference baseline
- Applies Sigma rules with fixed levels and no computed scores
- Filters report content by time window while running detection on all records
- Writes deterministic, atomic HTML, CSV and JSONL outputs

See [product](docs/product.md), [architecture](docs/architecture.md) and the current [plan](docs/plan.md) for details.

## Usage
```sh
cargo build --release   # binary: target/release/tool

# Build a baseline from a VanillaWindowsReference CSV
tool baseline build --vwr <vwr.csv> -o win11-24h2.fst

# Analyze an $MFT and optional $J
tool analyze -i <$MFT> -o report.html --csv out.csv --jsonl out.jsonl \
  --usn <$J> --baseline win11-24h2.fst --rules testdata/rules

# Focus the report on a time window
tool analyze -i <$MFT> --usn <$J> -o report.html --from 2026-09-28 --to 2026-10-02 --tz +09:00
```

Development and contribution guidance: [AGENTS.md](AGENTS.md).

## License
- Code: [AGPL-3.0-only](LICENSE)
- Detection rules: [DRL 1.1](https://github.com/SigmaHQ/Detection-Rule-License)
