# ADR 0019: Sample rules stay generic; no third-party-data rules

Status: accepted (2026-10-10, approved by the maintainer)

## Context
P1-9 added rules named after products (BloodHound, seven remote-access tools), and P1-10 (ADR 0017, ADR 0018) generated 307 rules from LOLRMM. The maintainer reviewed them and found two problems. Rules tied to one product's file names are too narrow to be worth keeping. The LOLRMM rules also brought third-party license text (Apache-2.0) into this repo.

## Decision
- Rules in this repo match generic facts only: location, extension, system-binary names and `service: baseline_outside`. A rule must not list product or tool names.
- No rule is generated from third-party data, and no third-party license text for rule data is kept in this repo.
- The following are removed: `testdata/rules/lolrmm_*.yml`, `LICENSE-LOLRMM`, the LOLRMM `NOTICE` entry, `cargo run -p xtask -- lolrmm`, `bloodhound_collection_output.yml` and `remote_access_tool_artifact.yml`.
- Kept from ADR 0018: `sigma` loads all rules in one batch. This is independent of where the rules come from, and a larger rule set (the Phase 2 SigmaHQ import) needs it.
- Product-specific knowledge needs a new ADR for the Phase 2 `tool-rules` repo.

This ADR supersedes ADR 0017 and ADR 0018.

## Consequences
- `testdata/rules/`: the 7 generic rules remain.
- `crates/xtask`: the `lolrmm` subcommand and its `serde_json` dependency are removed.
- `crates/cli/tests/analyze.rs`: the tests for the removed rules are removed.
- `NOTICE`: the "Third-party data" section is removed.
- `docs/plan.md`: P1-11; P1-10 is marked superseded.
- Git history still contains the removed files. Apache-2.0 allows that, and history is not rewritten.
