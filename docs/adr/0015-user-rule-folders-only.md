# ADR 0015: Normalization rule 1 applies to profile folders only

Status: accepted (2026-10-07, approved by the maintainer)

## Context
ADR 0012 rule 1 replaces segment 2 under `USERS` with the `USER` placeholder. That includes a file sitting directly in `\Users`.

If a baseline ever lists such a file (for example a `desktop.ini` directly in that folder), its key becomes `USERS\␀USER`. A planted `\Users\evil.exe` then gets the same key and is labeled `standard`.

None of the three VWR builds in use (Win10 22H2, Win11 22H2, Win11 24H2) lists a file there today. The P1-3 review found the gap.

## Decision
- **Rule 1 applies only when the user-name segment has something below it**, i.e. it names a profile folder.
- A file directly under `\Users` keeps its literal name.
- **The normalization version goes from 1 to 2.** Baseline files built under version 1 are refused (ADR 0012).

This ADR supersedes ADR 0012 rule 1 only; the other rules are unchanged.

## Consequences
- `crates/baseline/src/normalize.rs`: the rule 1 condition and `NORMALIZATION_VERSION = 2`.
- S4 numbers re-measured with the real code, with no change:
  - Win11 22H2: 214,262 files, 70.7 % outside.
  - Win10 22H2: 63,350 files, 82.5 % outside.
- ADR 0012's status line points here.
