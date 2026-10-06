# ADR 0013: Coverage-guided fuzzing in CI

Status: accepted (2026-10-06, approved by the maintainer)

## Context
ADR 0004 rule 3 and `.claude/rules/rust.md` require a `cargo fuzz` target per parser entry point. Spike S2 used proptest on stable instead (option b). `cargo-fuzz` needs a nightly toolchain and `libfuzzer-sys`, which compiles C++ (libFuzzer) in its build script. The workspace pins stable 1.98.1.

## Decision
**A separate `fuzz/` workspace on a pinned nightly, run on a schedule in CI. Never on the release path.**

| Item | Choice |
|---|---|
| Layout | `fuzz/` with its own `Cargo.toml` and `Cargo.lock`, excluded from the main workspace; one target per parser entry point (first: `mft-parse` FILE record) |
| Toolchain | `nightly-YYYY-MM-DD` pinned in `fuzz/rust-toolchain.toml`; the main `rust-toolchain.toml` stays stable |
| Dependency | `libfuzzer-sys =0.4.13` |
| CI | Workflow `fuzz.yml`: weekly schedule + `workflow_dispatch`, 10 min per target, `contents: read` only, actions pinned by SHA (ADR 0007) |
| Corpus | Not committed (`/fuzz/corpus/` is git-ignored). Seeds come from the test builder at run time |
| Crashes | Uploaded as a workflow artifact (3-day retention). A crash becomes a regression test in the parser crate; the fuzz job never blocks PRs |
| Local | `cargo +nightly-YYYY-MM-DD fuzz run <target>` on the Mac |

**Dependency vetting** (2026-10-06):
- `libfuzzer-sys =0.4.13` adds 10 crates. Maintainers are the rust-fuzz team (`frewsxcv`, `fitzgen`, `nagisa`, `Manishearth`). Released since 2017, 68 M downloads, last release 2026-06.
- License `(MIT OR Apache-2.0) AND NCSA`. **NCSA is not in the `deny.toml` allow-list.** It is permissive and GPL-compatible, so it needs to be added.
- `build.rs` compiles the bundled libFuzzer C++ sources with `cc`. It honours `CUSTOM_LIBFUZZER_PATH`. No network access.
- Transitive crates: `arbitrary`, `cc`, `jobserver`, `shlex`, `find-msvc-tools`, `cfg-if`, `libc`, `getrandom`, `r-efi`.

Not chosen:
- proptest only: no coverage feedback, so it misses deep parser states.
- Fuzzing on every PR: slow, and nightly breakage would block unrelated work.

## Consequences
- `deny.toml`: add `NCSA` to `[licenses] allow`; run `cargo deny check` in `fuzz/` too.
- `cargo vet`: the `fuzz/` lockfile is outside the main workspace and is vetted at the H2 step like the spikes. Its crates never ship.
- Root `Cargo.toml`: add `fuzz` to `exclude`.
- P1-1 adds the first target and the workflow. A human installs `cargo-fuzz` (`cargo install --locked cargo-fuzz`).
