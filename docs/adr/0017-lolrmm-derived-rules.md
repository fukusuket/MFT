# ADR 0017: Generate remote-access-tool rules from LOLRMM

Status: proposed (2026-10-09)  <!-- a human changes this to accepted (AGENTS.md H2) -->

## Context
P1-9's hand-written remote-access-tool rule covers 7 product names. Attackers use hundreds of RMM tools and RATs (Atera, Splashtop and AnyDesk appeared together on one real Win11 host). LOLRMM (https://github.com/magicsword-io/LOLRMM) lists 356 such tools (260 RMM, 96 RAT): 318 with install paths, 64 with PE file names, 99 with Windows disk artifacts. It is **Apache-2.0** (checked 2026-10-09), so under ADR 0006 it may become rule content, like LOLDrivers.

LOLRMM also ships Sigma rules, but they are uneven. For example, the Atera file rule lists only `C:\Program Files\ATERA Networks\…`, so it misses the `Program Files (x86)` install seen on that host.

## Decision
| Topic | Decision |
|---|---|
| Use | LOLRMM data may be turned into DRL rules (extends the ADR 0006 table; ADR 0006 is not superseded). LOLRMM's own Sigma rules are not imported. |
| Source | `website/public/api/rmm_tools.json` at a pinned commit (first: `dc6ebe934bce`). A refresh is a reviewed change that bumps the commit and regenerates. |
| Fields | `Details.InstallationPaths`, `Details.PEMetadata[].Filename`, `Artifacts.Disk[].File` where `OS` is Windows. |
| Generator | `cargo xtask lolrmm <rmm_tools.json> <out-dir>`, offline: the JSON is fetched by a human or agent, never by the tool. It uses `serde_json` (already vetted, ADR 0010). It is a new dependency edge for `xtask` only. |
| Normalization | Placeholders such as `(Random)` and `<…>` become `*`. `C:\Program Files\` also yields the `(x86)` variant, and the reverse. A bare file name becomes `TargetFilename\|endswith: '\<name>'` and is kept only if it has an extension. Patterns that match almost anything (`*`, `*\*`, a drive root) are dropped. Output is sorted and deduplicated. |
| Rules | One rule per tool: `lolrmm_<slug>.yml`, title `<Name> file (LOLRMM)`, `level: medium` for RMM and RAT alike (ADR 0016: legitimate use is common, and a file name alone is the evidence), `references` to the LOLRMM page, and an `id` derived deterministically from the tool name, so regeneration keeps ids stable. |
| Licensing | Each generated file starts with `Derived from LOLRMM (Apache-2.0, commit <sha>). Detection Rule License (DRL) 1.1.` The Apache-2.0 text is stored next to the rules, and `NOTICE` gets a third-party data entry. |
| Location | `testdata/rules/` until the Phase 2 `tool-rules` repo. The hand-written `remote_access_tool_artifact.yml` stays, because LOLRMM does not list installer names such as `ateraAgentSetup*.msi`. |

## Consequences
- `crates/xtask`: new `lolrmm` subcommand; `serde_json` added to its `Cargo.toml`.
- `testdata/rules/`: about 300 generated rules plus `LICENSE-LOLRMM`; `NOTICE`: LOLRMM entry.
- `docs/plan.md`: P1-10.
- Determinism: the same JSON gives byte-identical rule files.
