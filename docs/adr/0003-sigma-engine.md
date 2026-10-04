# ADR 0003: Sigma engine — `rsigma-eval`

Status: accepted (2026-10-04, approved by the maintainer)

## Context
Spike S1 (docs/plan.md) had to show whether `rsigma-eval` can run our Sigma rules on NTFS events before we build the `sigma` adapter crate; the fallback was extracting Hayabusa's engine. Evidence and commands: [spikes/s1-rsigma/README.md](../../spikes/s1-rsigma/README.md).

## Decision
**Adopt `rsigma-eval` and `rsigma-parser`** behind our own `sigma` adapter crate.

| Spike criterion | Result |
|---|---|
| `Event` on an NTFS type, no JSON | Pass |
| ~120 rules × 1M events ≤ 10 s | Pass: 88 usable SigmaHQ file rules × 1M events in 0.43–0.47 s |
| `temporal_ordered` with USN times | Pass (with rule `id`s, see below) |
| Deterministic | Pass (identical digest over two runs, after sorting) |
| `explain` as evidence | Pass (field, matcher, pattern, actual value per item) |
| `service: baseline_outside` routing | Pass |

**Dependency** (vetting from the H2 step, 2026-10-04): `=0.23.0`, `default-features = false` (the default `fix` feature pulls `tree-sitter`/`tree-sitter-yaml`, which compile C). MIT; 59 transitive crates (91 for all targets); build-time code only in widely used crates (serde, thiserror, ahash, crc32fast, zmij, …); no C compilation. Single crates.io owner and a release almost every week: pin with `=`, update deliberately.

**Rules for the adapter**
1. Every rule must have an `id`: `process_with_detections` links detections to correlations by `id` only, and without one a correlation silently never fires. Lint this in `tool-rules` CI.
2. Route by logsource ourselves: `evaluate_with_logsource` per event, then `process_with_detections(event, detections, secs)`. Never `process_event_at` alone (it ignores logsource).
3. Correlation time is whole seconds. Convert from `Filetime` in the adapter; events keep full precision.
4. Use `explain_rule` for report evidence (not `matched_fields`, which repeats a field once per matcher).
5. Sort results ourselves (ADR 0002 #3).

Not chosen: extracting Hayabusa's engine (more work, coupled to EVTX records). It stays the fallback if rsigma's API churn becomes costly.

## Consequences
- New dependency for the `sigma` crate when it is created (Phase 1); `cargo vet` audits/exemptions for the new crates at that time.
- `docs/research/oss-reuse.md` and `docs/architecture.md` already name `rsigma-eval`; mark "pending ADR 0003" as decided.
- The SigmaHQ estimate of ~120 usable rules is 88 at commit `ca24243`; update `docs/product.md`/`oss-reuse.md` wording when the rule set is built.
