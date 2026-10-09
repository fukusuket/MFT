# ADR 0018: LOLRMM rule generation details

Status: accepted (2026-10-09, approved by the maintainer)

## Context
Implementing ADR 0017 (P1-10) on the real LOLRMM data showed gaps in its Normalization and Rules rows:
- Some paths are bare Windows names (`svchost.exe` for Overlord, `quickassist.exe`) or generic globs (`*Agent.exe`, `C:\Windows\*.exe`). Under ADR 0017 they would match every Windows host.
- Some paths are written with `\\` or use `[GUID]`, and `%` also appears as plain text (`Status%4Operational.evtx`).
- Plain glob values cost about 7 s per host. rsigma cannot prefilter a glob with an inner wildcard, so it evaluated every rule on every event. Separately, adding one rule file at a time rebuilt the engine's rule index each time.
- ADR 0017 names `cargo xtask lolrmm`, but the repo has no `xtask` alias.

This ADR adds to ADR 0017. It does not replace ADR 0017's decision.

## Decision
| Topic | Decision |
|---|---|
| Command | `cargo run -p xtask -- lolrmm <rmm_tools.json> <out-dir>`. It replaces every `lolrmm_*.yml` in `<out-dir>` and leaves other files alone. |
| Separators and placeholders | A run of `\` is one separator. `%NAME%` (a variable-name pattern only), `<…>`, `[…]` and every `(Random)` become `*`. Any other `%` is text. |
| User profiles | `C:\Users\<name>\` becomes `C:\Users\*\` unless `<name>` is `Public`, `Default`, `Default User` or `All Users`. Example names in the data (`IEUser`) match any profile, and no user name reaches a rule (the `personal-home-path` secret scan). |
| Too common to name a tool | A pattern is kept only if it has a folder segment that is not a Windows default (`Program Files[ (x86)]`, `ProgramData`, `Users`, `AppData`, `Local[Low]`, `Roaming`, `Windows`, `System32`, `SysWOW64`, `Temp`, `Microsoft`, `Start Menu`, `Programs`, `Startup`). Without such a folder, the file name must have ≥ 6 literal alphanumerics if it has a wildcard; otherwise it needs a ≥ 3-character stem and must not be on a short list of Windows and generic names (`svchost.exe`, `mstsc.exe`, `quickassist.exe`, `agent.exe`, `setup.exe`, …; kept in `crates/xtask/src/lolrmm.rs`). As a result, built-in tools such as Quick Assist get no LOLRMM rule. |
| Rule shape | Values are grouped into `exact`, `starting` (`\|startswith`), `ending` (`\|endswith`) and `containing` (`\|contains`) selections. A glob with an inner wildcard is its own selection, with its longest literal piece added as `\|contains`. Matching is unchanged, and rsigma can prefilter every rule. `condition: 1 of them`. |
| References | The LOLRMM repository URL. The site has no stable per-tool URL in the data. |
| File names | A tool name that maps to no file name, or to the same file name as another tool, is an error. |
| Rule loading (`sigma`) | All rule files are parsed first, then added in one `add_rules` batch, so the engine index is built once. |

## Consequences
- `crates/xtask/src/lolrmm.rs` and `crates/sigma/src/lib.rs` implement this; P1-10 cites both ADRs.
- On the 10 local hosts, findings are byte-identical to the plain-glob shape, and time per host went from about 7 s to about 0.4 s.
- Adding names to the "too common" list is a reviewed change, like a refresh.
