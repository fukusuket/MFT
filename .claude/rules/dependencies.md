---
paths:
  - "**/Cargo.toml"
  - "deny.toml"
  - "NOTICE"
---

# Dependencies and licensing

- New crates: `[lints] workspace = true` and inherit `[workspace.package]`.
- Crate dependency direction is fixed (docs/architecture.md). No reverse or cyclic edges; `sigma` never depends on NTFS crates.
- A new dependency is a human decision (H2). Before proposing it, verify the exact crate name on crates.io (no typosquats or hallucinated names) and state: reason, license, maintainers, release history, whether it (or any new transitive crate) has `build.rs` or is a proc-macro. Those run code at build time and get explicit review.
- Must be AGPL-3.0 compatible; `cargo deny check` must pass. From the first dependency on, `cargo vet` must pass (ADR 0007 P5).
- Never run `cargo build` on a newly added crate before the human approves it. Build with `--locked` in CI.
- Code copied or ported from another project: add source, license and commit to `NOTICE`.
- GPL-3.0 data (LOLBAS, HijackLibs, winbindex) stays AGPL-side data; never compile it into DRL rules.
