# ADR 0021: USN parsing and Rewind are our own code

Status: accepted (2026-10-10, approved by the maintainer)

## Context
`oss-reuse.md` and `architecture.md` planned to port `$J` parsing and Rewind from `ntfs-core` (Apache-2.0). `ntfs-core` stores names with `from_utf16_lossy` (against ADR 0002), has a single maintainer, and porting would bring third-party code and a `NOTICE` entry into the repo (compare ADR 0019). The record layout and the Rewind idea are both publicly documented.

## Decision
| Area | Decision |
|---|---|
| `usn-parse` (P2-1) | Written from Microsoft's `USN_RECORD_V2` / `USN_RECORD_V3` / `USN_RECORD_V4` documentation; V4 is reported, not parsed |
| Rewind (P2-3) | Written from the published algorithm (CyberCX, 2024) |
| Oracles | usnjrnl_rewind (MIT, Python) for record counts and paths; `ntfs-core` optional. Both are installed and run outside the repo (scratchpad), never vendored, never a dependency |
| Third-party code | None copied; `NOTICE` gains nothing |

## Consequences
- `docs/research/oss-reuse.md`: the `$J` parsing row becomes "own implementation; oracles only".
- `docs/architecture.md`: the `usn-parse` and `resolve` rows drop "ported from `ntfs-core`".
- `docs/plan.md`: the ADR item is ticked.
- No new dependency; `deny.toml`, `supply-chain/` and `NOTICE` are unchanged.
