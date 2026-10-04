# ADR 0001: License

Status: accepted (2026-10-04)

## Decision

| Scope | License |
|---|---|
| Code (all crates, viewer, collector) | AGPL-3.0-only, same family as Hayabusa (verified on GitHub 2026-10-04) |
| Detection rules | DRL 1.1, same as SigmaHQ and hayabusa-rules |
| Baseline data | TBD |

`-only` is provisional because Hayabusa's `Cargo.toml` has no `license` field. If changed, update `Cargo.toml` and `NOTICE`.

## Consequences
- GPL-3.0 code may be incorporated; record the source in `NOTICE`.
- Dependencies must be AGPL-3.0 compatible; enforced by `deny.toml`. No GPL-2.0-only, LGPL-2.1-only, SSPL, BUSL.
- The HTML report embeds AGPL viewer code, so its footer must show the license and source URL with commit.
- DRL 1.1: output of rule matches (report, JSONL, CSV) must include the rule `author` and reference.
- GPL-3.0 data must not be compiled into DRL rules (see docs/plan.md, ADR 0006).
- AGPL §13 does not affect a local CLI; revisit if a server mode is added.
