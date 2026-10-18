# ADR 0024: Collection zip format and `rawzip`

Status: accepted (2026-10-17)

## Context
`mozu collect` (plan P3-1, product.md M1) writes the four NTFS files of a live volume into one zip; Phase 4 (P4-1) reads it, and zips from KAPE and Velociraptor, in `analyze`. The zip and its `meta.json` are therefore a format, and reading zips is a trust boundary (zip bombs, odd entry names). `collector` uses `ntfs-reader` (ADR 0005) and must not depend on analysis crates. product.md §7 Q4 leans "sign the collector from MVP".

## Decision

**Dependency: `rawzip =0.5.2`** (writer in `collector` now, reader in `analyze` in P4-1), with `flate2 =1.1.10` for DEFLATE.

| | `rawzip` 0.5.2 | `zip` 9.0.4 (not chosen) |
|---|---|---|
| License | MIT | MIT |
| Dependencies | none | `crc32fast`, `indexmap`, `memchr`, `typed-path`, `nix` (macOS) required; compression optional |
| New crates in our tree | 1 | 4–5 |
| `build.rs` / proc-macro | none / no | none in `zip`; `nix` has a `build.rs` |
| `unsafe` | none (`grep`: only in comments) | 26 sites, plus `typed-path`, `nix` |
| Zip64 | read and write | read and write |
| Owners / releases | 1 (nickbabcock) / 19 since 2025-01-30, none yanked, last 2026-10-04 | 2 / 136 (17 yanked), five in 2026-10-09..16 (9.0.0–9.0.4) |

- `flate2` 1.1.10, `miniz_oxide` 0.9.1, `adler2` and `crc32fast` 1.5.2 are already in `Cargo.lock` through `rsigma-eval` and exempted in `supply-chain/config.toml`; `flate2` is used with its default pure-Rust backend. The only new crate is `rawzip`.
- Pin with `=`: one owner. Fallback: `zip` with `default-features = false, features = ["deflate"]`.

**Zip layout** (written by `collector`)

| Entry | Content |
|---|---|
| `$MFT` | `$MFT` `$DATA`, as read |
| `$J` | Stored extents of `$UsnJrnl:$J` only (ADR 0005 #1) |
| `$Boot` | `$Boot` |
| `$SDS` | `$Secure:$SDS` |
| `meta.json` | Below; written last |

- Flat, fixed names; DEFLATE for every entry; Zip64 when needed (a `$MFT` can exceed 4 GiB). Entry times are the collection start time.
- Default output: `<COMPUTERNAME>_<UTC yyyymmddThhmmssZ>.zip` in the current directory; never overwrites; written to a temporary name and renamed when complete.
- `mozu collect` exists only on Windows builds; elsewhere the subcommand prints that it runs on Windows.

**`meta.json`**: only what the files cannot tell.

| Field | Value |
|---|---|
| `format` | `1`; a reader refuses another value |
| `tool` | `mozu` version and commit |
| `host` | `COMPUTERNAME` (display only; P4-18 `Computer`) |
| `volume` | e.g. `C:` |
| `started`, `finished` | UTC, ISO 8601 |
| `files` | Per entry: name, size written |
| `j` | `first_offset` (logical offset of the first stored byte = its USN), `stored`, `sparse`, `lost` bytes (ADR 0005 #2) |

Not in `meta.json`:
- **OS build, UBR, edition, language**: Phase 4 infers them from `$MFT` for every input (ADR P4-0b), so KAPE and Velociraptor input works the same way; one way to do a thing.
- **SHA-256 per file**: each entry's CRC32 detects transfer damage and is checked on read; a per-file hash inside the same zip adds no tamper evidence and would add `sha2`. For chain of custody the user hashes the zip with OS tools (`Get-FileHash`), as the manual (P4-21) says.

**Reading a zip** (P4-1; fixed here so the writer and reader agree)
- Read only `$MFT`, `$J` (KAPE and Velociraptor names included) and `meta.json`; ignore other entries. Never create files named by an entry; data goes to files named by the tool.
- Accept stored and DEFLATE; refuse encrypted entries and other methods with a message.
- Each entry is read through `rawzip`'s verifying reader: decompressed size and CRC32 must match the central directory, so a bomb stops at its declared size. Refuse more than 100,000 entries and any entry whose declared size exceeds 64 GiB.
- Treat entry names as attacker input (ADR 0002 #4) in messages.

**Code signing**: Phase 3 builds are unsigned test builds; the release binary is signed (Authenticode) from the v0.4 MVP release. The signing provider (e.g. SignPath Foundation for OSS, Azure Artifact Signing) and the certificate holder are the maintainer's choice, recorded in a later ADR before v0.4. Signing is a release step done by a human, not by agents.

## Consequences
- New crate `collector` (`cfg(windows)` for `ntfs-reader`; the zip and `meta.json` writer are platform-independent and unit-tested on every OS): `rawzip = "=0.5.2"`, `flate2 = "=1.1.10"`, `serde_json` (ADR 0010).
- `supply-chain/config.toml`: exemption for `rawzip` 0.5.2; `cargo deny check` passes (MIT).
- `docs/plan.md`: ADR P3-0 points here; the Phase 3 gate and P3-G drop "SHA-256 in `meta.json`" for "CRC32 checked, sizes match `meta.json`"; ADR P4-0b drops "`meta.json` wins".
- `docs/architecture.md`: `collector` row (Built on `ntfs-reader`, `rawzip`, `flate2`); `docs/research/oss-reuse.md`: zip choice.
- `NOTICE`: nothing (no code copied).
