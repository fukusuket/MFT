# ADR 0014: Overflow checks off for the `mft` crate

Status: proposed (2026-10-07)  <!-- a human changes this to accepted (AGENTS.md H2) -->

## Context
P1-1's proptest (`crates/mft-parse/tests/no_panic.rs`, corrupted valid records) found a panic in `mft` @ `18b6c05`:
- `attribute/data_run.rs`: `decode_run_value` shifts by 64 when a run's size nibble is 0, and `decode_data_runs` adds `i64` offsets without checking.
- Both are reached when a corrupted attribute is marked non-resident.

This repo builds with `overflow-checks = true` (root `Cargo.toml`, release), and debug/test builds have it on by default, so these become panics. The S2 spike ran in a standalone crate with default release settings (no overflow checks), so they wrapped silently and the "0 panics" result in ADR 0004 missed them.

Filtering to `$SI`/`$FN` with `iter_attributes_matching` is not enough: a crafted non-resident `$FN` still reaches the decoder.

## Decision
**Disable overflow checks for the `mft` package only, in the release and dev profiles.**

```toml
[profile.release.package.mft]
overflow-checks = false

[profile.dev.package.mft]
opt-level = 2
overflow-checks = false
```

- Effect: the affected arithmetic wraps instead of panicking. `mft-parse` reads no data runs, so wrapped values never reach our output.
- Our own crates keep overflow checks.
- Evidence: 300,000 release and 100,000 dev proptest cases passed after the change; the same release run panicked before it.
- Upstream: a human reports the bug with the proposed two-line fix (draft prepared by the agent). When a fixed release ships, remove the overrides together with the git pin (ADR 0004).

Not chosen:
- Forking `mft`: needs a hosted fork (a human internet write) for a two-line fix.
- Vendoring a patched copy: a large amount of third-party code to own.
- `catch_unwind`: it hides the cause, and panic messages still reach stderr.

## Consequences
- Root `Cargo.toml`: the two profile overrides above.
- ADR 0004 still stands; its "no panic" row holds only with this ADR.
- `fuzz/` (ADR 0013) needs the same override in its own workspace manifest.
- Any later use of `mft` data runs (non-resident attributes) must validate them or wait for the upstream fix.
