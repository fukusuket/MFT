---
name: spike
description: Run a Phase 0 spike (S1-S4 in docs/plan.md) as throwaway code and record the result. Use when asked to evaluate a library or approach before committing to it.
disable-model-invocation: true
---

Spike: $ARGUMENTS

1. Read the spike's row in `docs/plan.md` (what to verify, pass criteria, fallback).
2. Create `spikes/<name>/` as a standalone crate (`[workspace]` table in its `Cargo.toml`); it is excluded from the main workspace.
3. Implement only what is needed to check each criterion. Print measured numbers.
4. Run it and capture the evidence (command, output, timings).
5. Write the outcome as a proposed ADR with the `adr` skill (pass → adopt; fail → fallback), quoting the evidence.
6. Stop for human acceptance (H2). After acceptance, tick the spike in `docs/plan.md`. Do not move spike code into `crates/`.
