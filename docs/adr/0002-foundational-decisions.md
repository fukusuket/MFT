# ADR 0002: Foundational decisions

Status: accepted (2026-10-04)

| # | Decision | Reason |
|---|---|---|
| 1 | Evidence file names are `NtfsName` (raw UTF-16). Never use `std::path::Path` for evidence paths | Unpaired surrogates must survive; `Path` follows the analysis host's OS rules |
| 2 | Compare paths by `NormPath`: normalized and upper-cased with `$UpCase`; same function for baseline build and analysis | NTFS is case-insensitive; `fst` lookups are exact |
| 3 | Sort all output by explicit keys; same input → byte-identical output, checked in CI | Reproducibility is a core product property; `rayon` and `HashMap` reorder results |
| 4 | Treat file names as attacker input: Base64-embed report data, neutralize CSV formulas, escape control characters in terminal output | XSS, CSV injection, terminal spoofing |
| 5 | Split into small crates with a fixed dependency direction; `sigma` does not depend on NTFS crates | Contain change; keep the Sigma engine replaceable |
| 6 | `unsafe_code = "forbid"` workspace-wide; only a collector FFI module may be exempt (may become unnecessary, see ADR 0005) | Parsers handle untrusted input |
| 7 | Time is UTC `Filetime(u64)` internally; time zones only in report/cli | No precision loss; one conversion point |
| 8 | Separate fatal `Error` from per-record `Diagnostic`; parsers skip bad records and report them | Corruption must not stop the analysis |
