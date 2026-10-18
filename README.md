<p align="center"><img src="mozu.png" alt="mozu" width="480"></p>

# Mozu

**Find what to look at on a Windows host, even after the event logs were wiped.**

mozu reads NTFS metadata (`$MFT` and `$UsnJrnl:$J`), hides files that a clean Windows also has, runs Sigma rules and writes one HTML report you open in a browser.

> Pre-alpha. Phase 2 of the [plan](docs/plan.md). Interfaces may change.

## What you get

| | |
|---|---|
| **Findings** | Files and file events that match a Sigma rule, ordered by level, then time. No scores |
| **Outside-baseline files** | Files a clean Windows does not have, even when no rule matched |
| **Deleted and renamed files** | Paths rebuilt from the USN journal |
| **One HTML file** | Works offline; no install, no network |
| **CSV / JSONL** | Every row, for `jq`, Timeline Explorer or a spreadsheet |

Same input → byte-identical output.

## What it cannot tell you

- Who or which process created a file, or whether it ran
- File content
- Activity older than the USN journal
- "Outside baseline" is a lead, not proof of compromise

## Not yet

`mozu collect` (Phase 3). Zip or collection-directory input, ready-made baselines, correlation rules, Timeline and File detail screens, Japanese UI (Phase 4). USN V4 records are reported, not parsed.

## Docs

| | |
|---|---|
| [Use cases](docs/usecases.md) | Step-by-step recipes |
| [Product](docs/product.md) | Problems, users, MVP |
| [Architecture](docs/architecture.md) | Pipeline, output columns, rule support |
| [Plan](docs/plan.md) | Current phase and next items |
| [AGENTS.md](AGENTS.md) | Development rules |

## License

- Code: [AGPL-3.0-only](LICENSE)
- Detection rules: [DRL 1.1](https://github.com/SigmaHQ/Detection-Rule-License)
