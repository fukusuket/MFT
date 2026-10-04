---
paths:
  - "crates/**"
---

# TDD cycle (every behavior change)

One cycle = one behavior. Split a task until each step has exactly one failing test.

1. **Red**: write one test for the next behavior. Run it and show it fails *for the expected reason* (an assertion, not an unrelated compile error). No production code yet.
2. **Green**: write the *minimum* code that passes. No code that no failing test demands. Run the crate's tests; all must pass. Never edit or weaken an existing test to get green.
3. **Refactor**: mandatory, even when the answer is "nothing to change". Tests stay green; behavior does not change. Check:
   - code this cycle added that no test exercises → delete it
   - duplication, unclear names, functions doing more than one thing
   - unnecessary `pub`, clones, allocations, abstractions
   - `rust.md` rules (no panics, `NtfsName`, ordering)
   Then re-run tests and `cargo clippy -p <crate> --all-targets -- -D warnings`.
4. **Report** per cycle: test name, failing output (Red), passing output (Green), what was refactored or `no refactor: <reason>`.

## Commits
- Red + Green together as `feat:`/`fix:`; refactoring as a separate `refactor:` commit.
- A `refactor:` commit never changes test expectations.

## Exceptions
- `spikes/` is exploratory: no TDD.
- Pure renames/moves are refactor-only cycles: green before and after.
