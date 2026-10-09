# ADR 0016: Rule levels by false-positive tolerance

Status: proposed (2026-10-09)  <!-- a human changes this to accepted (AGENTS.md H2) -->

## Context
NTFS metadata gives rules only paths, names and times, so narrow rules miss true positives. Missed files cannot be recovered later; false positives can be sorted by level. The primary user is a non-expert IT admin (product.md §2), and Summary shows critical/high counts. A P1-8 run on 9 Win7 hosts showed 6 high false positives per host, which a non-expert would read as a compromise.

## Decision
| level | False positives | Who acts |
|---|---|---|
| critical / high | Near zero | The primary user, directly; counted in Summary |
| medium | Some | Review |
| low / informational | Unlimited | Experts, via CSV/JSONL |

- Rules cover a behavior broadly. They are not narrowed only to reduce noise; noise is separated by `level`.
- Prefer `service: baseline_outside` to remove OS-standard files without narrowing a rule.
- False-positive counts on real data are reported, not used to drop a rule. A high/critical false positive is fixed by an exclusion or a lower level.

## Consequences
- Rule review checks each rule's `level` against this table.
- `testdata/rules/` levels follow it from P1-9; the Phase 2 `tool-rules` repo inherits it.
