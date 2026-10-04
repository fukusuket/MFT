# ADR 0008: First dependency — `proptest` (dev) and `cargo-vet`

Status: accepted (2026-10-04, approved by the maintainer)

## Context
`NtfsName` must round-trip *any* `u16` sequence (unpaired surrogates included), and `.claude/rules/rust.md` requires property tests that arbitrary bytes never make a parser panic. Table tests cannot cover that input space. This is the workspace's first dependency, which also triggers ADR 0007 P5 (`cargo-vet` from the first dependency).

## Decision

**Add `proptest` as a dev-dependency only, minimal features, exact version.**

```toml
# crates/ntfs-types/Cargo.toml (later: each crate that needs property tests)
[dev-dependencies]
proptest = { version = "=1.11.0", default-features = false, features = ["std"] }
```

| Check (`.claude/rules/dependencies.md`) | Finding (2026-10-04) |
|---|---|
| Name | `proptest` on crates.io, repo `github.com/proptest-rs/proptest` |
| License | MIT OR Apache-2.0 (all transitive crates MIT/Apache/Unlicense-compatible; `cargo deny` to confirm) |
| Maintainers | `Centril`, `AltSysrq`, team `proptest-rs:publish` |
| History | Since 2017; ~200 M downloads; releases 1.6 → 1.11 between 2024-12 and 2026-03; none yanked |
| Transitive crates | 15 with `features = ["std"]` vs 27 with defaults (drops `rusty-fork`, `tempfile`, `wait-timeout`, `rustix`, …) |
| Build-time code | No proc-macros. `build.rs` in `getrandom`, `libc`, `num-traits` (via `autocfg`), `zerocopy`: all only probe `rustc --version` / compile cfg probes into `OUT_DIR`; `libc` also runs `freebsd-version`/`emcc` only for those targets. No network, no reads outside the build, no writes outside `OUT_DIR` |
| Scope | Dev-dependency: never linked into shipped binaries |

Not chosen: a hand-written pseudo-random generator in tests. It has no shrinking, so failures are hard to read, and parsers will need proptest in Phase 1 anyway.

**Start `cargo-vet` with this change.**
- `cargo vet init`, import the public audit sets of Mozilla, Google, Bytecode Alliance and ZcashFoundation (`cargo vet import`); crates still unaudited after imports go into `supply-chain/config.toml` exemptions at their exact versions.
- New or updated crates later need an audit or an exemption reviewed in the PR.
- CI job: `cargo vet --locked`.

## Consequences
- A human installs `cargo-vet` (e.g. `cargo install --locked cargo-vet`, or a release binary) and approves this ADR before any `cargo build` with `proptest`.
- New: `supply-chain/` (`config.toml`, `audits.toml`, `imports.lock`); CI `vet` job; `cargo vet` added to AGENTS.md verify commands.
- `crates/ntfs-types/Cargo.toml`: dev-dependency above; `Cargo.lock` gains the 15 crates.
- `docs/plan.md`: tick "`cargo-vet` init".
- Dependabot `cargo` updates of these crates require a vet audit or exemption before merge.
