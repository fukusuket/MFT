---
paths:
  - "**/Cargo.toml"
  - "deny.toml"
  - "NOTICE"
---

# Dependencies and licensing

- New crates: `[lints] workspace = true` and inherit `[workspace.package]`.
- Crate dependency direction is fixed (docs/architecture.md). No reverse or cyclic edges; `sigma` never depends on NTFS crates.
- A new dependency needs a reason, its license and maintenance status in the PR. Must be AGPL-3.0 compatible; `cargo deny check` must pass.
- Code copied or ported from another project: add source, license and commit to `NOTICE`.
- GPL-3.0 data (LOLBAS, HijackLibs, winbindex) stays AGPL-side data; never compile it into DRL rules.
