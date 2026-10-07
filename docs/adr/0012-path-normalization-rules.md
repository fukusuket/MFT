# ADR 0012: Path normalization rules for the baseline

Status: accepted (2026-10-06, approved by the maintainer); rule 1 amended by [ADR 0015](0015-user-rule-folders-only.md)

## Context
Baseline lookups are exact (ADR 0002 #2). Paths differ between hosts in user names, SIDs, GUIDs and WinSxS version parts. Spike S4 measured a small set of rules ([baseline-poc.md](../research/baseline-poc.md)). The question is whether the rules live in Rust or in declarative YAML in `tool-rules`, and what a normalized key looks like. P1-3 needs the answer.

## Decision
**A small fixed table in the `baseline` crate for Phase 1. Move it to YAML when the baseline CI arrives (Phase 3) and a second consumer exists.**

Rules, applied to each segment of a `NormPath` (already upper-cased with `$UpCase`). They are the same as the S4 spike, so P1-3 can reproduce its ratio.

| # | Match | Replaced by |
|---|---|---|
| 1 | Segment 2 under `USERS`, except `PUBLIC`, `DEFAULT`, `DEFAULT USER`, `ALL USERS` | `USER` placeholder |
| 2 | Segment starting `S-1-5-` | `SID` placeholder |
| 3 | `_`-separated token that is a GUID (8-4-4-4-12 hex, optional braces) | `GUID` placeholder |
| 4 | `_`-separated token of three or more dot-separated decimal numbers | `VER` placeholder |
| 5 | `_`-separated token of 16 or more hex digits | `HEX` placeholder |

For rules 3–5, a trailing `.MANIFEST`, `.CAT` or `.MUM` is kept outside the token.

- **Placeholders start with U+0000**, for example `\0USER`. NUL can't occur in an NTFS name, so an attacker can't create a real file whose name matches a placeholder. With `%USER%` (legal in NTFS names), a directory named `%USER%` would turn a planted file into a "standard" one.
- **One function** builds both the baseline keys and the analysis keys (ADR 0002 #2).
- **Every baseline file stores a normalization version.** A mismatch with the analyzer is a fatal `Error`.
- **Reports show placeholders as `%USER%` etc.** Display only.

Not chosen:
- YAML now: a parser, a schema and a loader for one consumer (AGENTS.md: no abstraction before a second use).
- Language normalization (product.md §7 Q2): Phase 1 has a single en-US baseline.

## Consequences
- P1-3: rules as a table plus unit tests per rule, including `%USER%` and other look-alike names that must not normalize.
- `docs/research/baseline-poc.md` numbers are the reference for "S4 ratio reproduced".
- Phase 3: revisit the table → YAML in `tool-rules` with the baseline CI.
