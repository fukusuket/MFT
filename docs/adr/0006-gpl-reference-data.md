# ADR 0006: GPL-3.0 reference data stays out of DRL rules

Status: accepted (2026-10-04, approved by the maintainer)

## Context
Three useful datasets list which Windows binaries and DLLs belong where: LOLBAS (living-off-the-land binaries and their normal paths), HijackLibs (DLLs used for hijacking and their legitimate paths) and winbindex (Windows binary versions and paths). All three are **GPL-3.0** (re-checked 2026-10-04). Our rules are DRL 1.1 (ADR 0001). Rules generated from GPL data would be GPL derivatives distributed under another license. Two non-negotiables also apply: no detection logic in Rust, and only standard Sigma fields.

## Decision

| Dataset | License | Where it lives | How it is used |
|---|---|---|---|
| LOLBAS, HijackLibs, winbindex | GPL-3.0 | Data files shipped with the AGPL-3.0 code, never in `tool-rules` | Matched at runtime by the analyzer; results become **facts** and a **logsource**, never rule content |
| LOLDrivers | Apache-2.0 | May be turned into DRL rules directly | Normal Sigma rules (driver file names) |

**From data to findings, without detection logic in Rust**, using the same pattern as `service: baseline_outside` (docs/architecture.md):
1. The analyzer looks up each file name: is it a known LOLBin or hijackable DLL, and is its path one of the dataset's expected paths?
2. That gives facts: `known LOLBin`, `known hijackable DLL`, `outside expected path`. They are shown next to findings and never used to rank them.
3. Events whose name is known but whose path is not expected are also emitted under `logsource: service: unexpected_location`, with standard fields only (`TargetFilename`, …).
4. DRL rules decide level and wording, e.g. "LOLBin outside its expected path" = `service: unexpected_location` + `TargetFilename|endswith: '.exe'`. A rule never lists GPL data such as names or paths.

**Handling the data**
- Fetched by a script pinned to an upstream commit; stored with the upstream license text and commit; recorded in `NOTICE`.
- Updates are reviewed like dependency updates (H2 for a new dataset, PR review for refreshes).
- When a dataset's license allows it, it may move to the DRL side (via a new ADR).

## Consequences
- `docs/architecture.md`: add the `unexpected_location` logsource next to `baseline_outside` when the data is first imported.
- `.claude/rules/dependencies.md` already states the rule ("GPL-3.0 data … never compile it into DRL rules"); this ADR is its basis.
- Out of scope now: importing any dataset (Phase 2, plan item 2-6).
