@AGENTS.md

## Claude Code
- Path-scoped rules in `.claude/rules/` load automatically; `.rs` files are auto-formatted by a hook.
- Use plan mode for cross-crate, format or dependency changes. Skip it for one-sentence diffs.
- During iteration run checks per crate (`cargo clippy -p <crate> --all-targets -- -D warnings`, `cargo nextest run -p <crate>`); run the full set before reporting done.
- Before calling a plan item done, have a subagent review the diff against the item's "Done when" / "Out of scope" and report only (1) correctness gaps and (2) code the item does not require. Do not act on style suggestions.
- Skills: `/spike <S1-S4>` runs a Phase 0 spike; `adr` records a decision.
- When compacting, keep: current plan item, files changed, commands run and their results.
