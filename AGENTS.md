# AGENTS.md

Rust CLI that triages Windows NTFS (`$MFT`, `$UsnJrnl:$J`): subtract a Windows baseline, focus on a time window, run Sigma rules, write a self-contained HTML report. Pre-alpha; see `docs/plan.md` for the current phase.

## Non-negotiables
- **No computed scores.** No weights, sums or probabilities. A finding = matched Sigma rules with fixed `level`; order is level → time.
- **No detection logic in Rust.** Rules decide; code exposes facts.
- **Same input → byte-identical output.**
- **No network access** in analysis or reporting.
- **No internet writes by agents.** Pushing, publishing, PRs/issues/comments, uploads, non-GET requests, publishing artifacts and sending messages are human-only (enforced by `.claude/hooks/guard.sh`). Reading (docs, crates.io, GitHub GET) is fine.
- Inputs are `$MFT`, `$J`, `$Boot`, `$Secure:$SDS` only.
- **TDD:** Red → Green → Refactor for every behavior change; the refactor step is never skipped (`.claude/rules/tdd.md`).

## Scope discipline
- Do exactly the current plan item. Anything else goes in your report as a suggestion, not into code.
- No abstraction (trait, generic, config, feature flag) until a second real use exists.
- No new `pub` items, crates, CLI flags or dependencies unless the plan item names them.
- Don't touch files unrelated to the item (no drive-by renames or reformatting).
- Validate only at trust boundaries (evidence bytes, CLI args); don't re-check what types guarantee.
- Prefer deleting code to adding it. A smaller correct diff wins.

## Verify before you say done
```sh
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --no-tests=pass
cargo deny check
cargo llvm-cov --workspace --fail-under-lines 90   # untested lines = code nobody asked for
```
Install once: `cargo install --locked cargo-nextest cargo-deny cargo-llvm-cov`. Report the commands you ran and their result.

## Where to look
| Need | Read |
|---|---|
| What we build, MVP, CLI/UI | `docs/product.md` |
| Pipeline, crates, dependency direction | `docs/architecture.md` |
| Current phase, next tasks, done criteria | `docs/plan.md` |
| Why a decision was made | `docs/adr/` |
| Library choices | `docs/research/oss-reuse.md` |
| TDD cycle | `.claude/rules/tdd.md` |
| Rust coding and test rules | `.claude/rules/rust.md` |
| Dependency and license rules | `.claude/rules/dependencies.md` |
| Doc conventions | `.claude/rules/docs.md` |

Other agents: read the matching `.claude/rules/*.md` before editing files it covers (see its `paths:`).

## Human checkpoints
Work autonomously inside a plan item. Stop for a human only at:
- **H1 Start of an item**: present the approach and the list of behaviors/tests to write; wait for approval.
- **H2 Decisions**: write ADRs as `Status: proposed`; only a human sets `accepted`.
- **H3 End of an item**: hand over the branch with per-cycle reports, verify output and out-of-scope suggestions. A human reviews, merges to `main` and pushes. Never commit or merge on `main` (blocked by hook).
- **H4 Phase gates**: only a human ticks a gate in `docs/plan.md`.
- **H5 Exceptions**: stop and ask before changing an existing test's expectation or touching anything out of scope.

## Workflow
- Work on a branch per plan item (`<type>/<item>`, e.g. `feat/ntfs-types-fileref`).
- One purpose per change. Conventional Commits (`feat(resolve): ...`).
- Evidence files and corpora outside `testdata/` are read-only and never committed.
