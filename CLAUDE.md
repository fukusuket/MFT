@AGENTS.md

## Claude Code
- Path-scoped rules in `.claude/rules/` load automatically; `.rs` files are auto-formatted by a hook.
- Use plan mode for cross-crate, format or dependency changes. Skip it for one-sentence diffs.
- During iteration run checks per crate (`cargo clippy -p <crate> --all-targets -- -D warnings`, `cargo nextest run -p <crate>`); run the full set before reporting done.
- Before calling a phase item done, have a subagent review the diff against `docs/plan.md` and report only correctness or requirement gaps.
- Skills: `/spike <S1-S4>` runs a Phase 0 spike; `adr` records a decision.
- When compacting, keep: current plan item, files changed, commands run and their results.
