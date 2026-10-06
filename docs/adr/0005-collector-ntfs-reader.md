# ADR 0005: Collector — `ntfs-reader` for live raw-volume reads

Status: accepted (2026-10-06, approved by the maintainer)

## Context
`collector` must copy `$MFT`, `$UsnJrnl:$J` (without its sparse region), `$Secure:$SDS` and `$Boot` from a live Windows volume, with no `unsafe` in our code (ADR 0002). Spike S3 (docs/plan.md) ran `ntfs-reader =0.6.0` on a disposable GitHub `windows-latest` runner. Evidence and commands: [spikes/s3-ntfs-reader/README.md](../../spikes/s3-ntfs-reader/README.md). The fallback was `std::fs::File` on `\\.\C:` plus `ntfs-core`.

## Decision
**Adopt `ntfs-reader =0.6.0` (Windows only) behind `collector`.**

| Spike criterion | Result |
|---|---|
| Raw `$MFT` | Pass: 1.40 GB, 1,363,200 records in 7 s; all parse with `mft` @ `18b6c05` (0 corrupt, 0 unresolved paths) |
| `$J` without the sparse region | Pass: 36 MB written of 3.54 GB logical; 373,795 records, 0 malformed |
| `$Secure:$SDS`, `$Boot` | Pass (`$Boot` geometry matches the volume) |
| `$UpCase` | MD5 `7ff498a4…`, same as ADR 0009 |
| No `unsafe` in our code | Pass (`#![forbid(unsafe_code)]`) |
| 4Kn | Not tested (runner has 512-byte sectors); known limit. The crate aligns reads to 4096 bytes, so 4Kn is expected to work |

**Dependency** (vetted at the H2 step, approved 2026-10-05):
- License MIT OR Apache-2.0.
- No build script and no proc-macro.
- Dependencies: `windows` 0.62, `thiserror`, `time`, `tracing`.
- `unsafe` (about 20 sites) stays inside the crate for Win32 calls.
- It refuses to build on non-Windows targets, so `collector` must be `cfg(windows)`.

**Rules for `collector`**
1. Copy only stored extents of `$J`. Record the logical offset of the first stored byte in `meta.json`: a USN equals its offset in the logical stream (the first USN equalled the skipped length in S3).
2. Count sparse and lost bytes per stream and report them; never pad sparse regions.
3. Development is on macOS: check `collector` with `cargo check --target x86_64-pc-windows-msvc` and test it on the Windows runner.

Not chosen: `std::fs::File` + `ntfs-core` (more code to own for the same result); it stays the fallback.

## Consequences
- `collector` crate (Phase 3 `collect`): `[target.'cfg(windows)'.dependencies] ntfs-reader = "=0.6.0"`.
- `cargo vet`: exemptions for `ntfs-reader` and new transitive crates (`windows` family) when the dependency is added.
- `docs/architecture.md` "Built on" for `collector` and `docs/research/oss-reuse.md`: mark decided; note that `$J` sparse handling is verified.
- CI: a Windows job for `collector` when it exists.
