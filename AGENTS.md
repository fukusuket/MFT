# AGENTS.md

Rust CLI that triages Windows NTFS (`$MFT`, `$UsnJrnl:$J`): subtract a Windows baseline, focus on a time window, run Sigma rules, write a self-contained HTML report. Pre-alpha; see `docs/plan.md` for the current phase.

## Non-negotiables
- **No computed scores.** No weights, sums or probabilities. A finding = matched Sigma rules with fixed `level`; order is level → time.
- **No detection logic in Rust.** Rules decide; code exposes facts.
- **Same input → byte-identical output.**
- **No network access** in analysis or reporting.
- Inputs are `$MFT`, `$J`, `$Boot`, `$Secure:$SDS` only.

## Verify before you say done
```sh
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --no-tests=pass
cargo deny check
```
Install once: `cargo install --locked cargo-nextest cargo-deny`. Report the commands you ran and their result.

## Where to look
| Need | Read |
|---|---|
| What we build, MVP, CLI/UI | `docs/product.md` |
| Pipeline, crates, dependency direction | `docs/architecture.md` |
| Current phase, next tasks, done criteria | `docs/plan.md` |
| Why a decision was made | `docs/adr/` |
| Library choices | `docs/research/oss-reuse.md` |
| Rust coding and test rules | `.claude/rules/rust.md` |
| Dependency and license rules | `.claude/rules/dependencies.md` |
| Doc conventions | `.claude/rules/docs.md` |

Other agents: read the matching `.claude/rules/*.md` before editing files it covers (see its `paths:`).

## Workflow
- Explore and plan before cross-crate, format or dependency changes; record decisions as ADRs (`docs/adr/`).
- One purpose per change. Conventional Commits (`feat(resolve): ...`).
- Evidence files and corpora outside `testdata/` are read-only and never committed.
