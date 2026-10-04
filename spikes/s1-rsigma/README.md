# Spike S1: `rsigma-eval` on NTFS-shaped events

Throwaway. Result recorded in [ADR 0003](../../docs/adr/0003-sigma-engine.md).

## Run

```sh
# SigmaHQ rules at a pinned commit, outside the repo (DRL 1.1, not committed)
git clone --depth 1 --filter=blob:none --sparse https://github.com/SigmaHQ/sigma "$TMPDIR/sigma"
git -C "$TMPDIR/sigma" sparse-checkout set rules/windows/file      # commit ca24243a6e3f
python3 filter_rules.py "$TMPDIR/sigma/rules/windows/file" "$TMPDIR/s1-rules"   # see below
cargo run --release -- "$TMPDIR/s1-rules"
```

Rule filter: single-document rules in `file_event`, `file_delete`, `file_rename`, `file_change` whose detection uses only `TargetFilename`, `SourceFilename`, `CreationUtcTime`. Result: **88 of 189** (86 dropped for `Image`, the rest for other process fields or multi-document files).

## Results (macOS, Apple Silicon, rustc 1.98.1, release)

| Criterion | Result |
|---|---|
| `Event` on an NTFS type, no JSON | Pass. `FileEvent` implements `get_field` / `any_string_value` / `all_string_values`; `to_json` only for debug |
| ~120 rules × 1M events ≤ 10 s | Pass. **88 rules × 1,000,000 events: 0.43–0.47 s** (3 runs), 394 matches |
| `temporal_ordered` with USN times | Pass, once detection rules carry an `id` (see finding 1) |
| Deterministic after sorting | Pass. Two runs, identical digest `055d1c9e56fae5fc` |
| `explain` usable as evidence | Pass. Per item: field, matcher, pattern, actual value, matched/reason |
| `service: baseline_outside` routing | Pass. Rule with the service matches only tagged events; rules without it match both |

```
[perf] 88 rules x 1000000 events: 446.90ms (394 matches)
[determinism] run1 055d1c9e56fae5fc run2 055d1c9e56fae5fc equal=true
[routing] outside_baseline=false: ["Any executable (no service)"]
[routing] outside_baseline=true: ["Outside-baseline executable", "Any executable (no service)"]
[correlation] t= 1060 file_delete C:\Windows\Temp\a.exe    fired=["Executable created then deleted"]   (60 s)
[correlation] t= 5600 file_delete C:\Windows\Temp\b.exe    fired=[]   (1 h, outside 5 m)
[correlation] t= 6000 file_delete C:\Windows\Temp\c.exe    fired=[]   (no create)
[correlation] t= 7010 file_event  C:\Windows\Temp\d.exe    fired=[]   (wrong order)
```

`explain` excerpt for `C:\PerfLogs\x171000.exe`:
```json
{ "field": "TargetFilename", "matcher": "startswith", "pattern": "c:\\perflogs\\",
  "actual": "C:\\PerfLogs\\x171000.exe", "matched": true, "reason": "matched" }
```

## Findings
1. `process_with_detections` maps detections to correlations **by rule `id` only** (`find_rule_identity`); the README's "matched by title" fallback does not apply on this path. Without `id`, correlations silently never fire. → Our rules must always have an `id` (lint in `tool-rules` CI).
2. Logsource routing must be done by us: compute detections with `evaluate_with_logsource`, then feed `process_with_detections`. `process_event_at` alone ignores logsource, so a "create" rule would also match delete events.
3. Time is whole seconds (`i64`); FILETIME precision (100 ns) is lost inside correlation only. Event data keeps full precision.
4. `matched_fields` lists one entry per matcher (duplicates for the same field); the report should prefer the `explain` trace.
5. Evaluation is single-threaded here; `rayon` (`parallel` feature) not needed at this speed.
