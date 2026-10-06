# Spike S3: collect from a live Windows volume with `ntfs-reader`

Throwaway. Result will be recorded in ADR 0005.

## What it does
`s3-ntfs-reader.exe \\.\C: <out>` (elevated) opens the raw volume with `ntfs-reader =0.6.0` and writes:

| File | Source | Notes |
|---|---|---|
| `MFT` | record 0, default stream | raw `$MFT` |
| `J` | `$Extend\$UsnJrnl:$J` | only stored extents; sparse extents skipped and counted |
| `SDS` | record 9 (`$Secure`), stream `$SDS` | security descriptors |
| `Boot` | record 7 | boot sector |
| `UpCase` | record 10 | for the ADR 0009 MD5 check |

Our code has `#![forbid(unsafe_code)]`; all `unsafe` stays inside `ntfs-reader` (Win32 calls).

## Run
- macOS: `cargo check --target x86_64-pc-windows-msvc` (needs `rustup target add x86_64-pc-windows-msvc`).
- Windows runner: workflow `.github/workflows/spike-s3.yml` (`workflow_dispatch`). It builds, prints whether the job is elevated and the OS build, runs the spike on the runner's `C:`, and uploads `out/` as the artifact `s3-ntfs-reader-output` (kept 3 days).
- Then on the Mac: parse `MFT` with `spikes/s2-mft` and check the `UpCase` MD5.

## Results (2026-10-06, GitHub `windows-latest` runner, `C:` 160 GB, 4 KiB clusters, 512-byte sectors)
`run.log` from the artifact:
```
[volume] \\.\C:: cluster 4096 B, record 1024 B, size 160467762176 B
[mft] records 1363200, corrupt 0, loaded in 10.57s
[MFT] stream "(default)": logical 1395916800 B, written 1395916800 B, sparse skipped 0 B, lost 0 B, 6.99s
[Boot] stream "(default)": logical 8192 B, written 8192 B, sparse skipped 0 B, lost 0 B, 661.80µs
[SDS] stream "$SDS": logical 1441132 B, written 1441132 B, sparse skipped 0 B, lost 0 B, 194.36ms
[UpCase] stream "(default)": logical 131072 B, written 131072 B, sparse skipped 0 B, lost 0 B, 23.04ms
[J] stream "$J": logical 3542490816 B, written 36052672 B, sparse skipped 3506438144 B, lost 0 B, 2.06s
[done] 19.88s
```

Checks on the Mac:

| File | Check | Result |
|---|---|---|
| `MFT` | `spikes/s4-baseline` (`mft` @ `18b6c05`) loads every record and builds paths | 1,363,200 records, 0 corrupt; 1,129,580 in-use files, 0 unresolved paths; 10 s |
| `J` | Walk `USN_RECORD` headers; the first USN equals the skipped sparse length | 373,795 v2 records, 0 malformed; first USN 3,506,438,144 = sparse skipped |
| `Boot` | NTFS OEM ID, `55AA`, geometry matches `run.log` | Pass (512 B/sector, 8 sectors/cluster) |
| `SDS` | First entry header (hash, id `0x100`, offset 0, length) | Plausible; not parsed further |
| `UpCase` | MD5 | `7ff498a44e45e77374cc7c962b1b92f2` (ADR 0009) |

Not covered: 4Kn volumes (the runner has 512-byte sectors), client Windows (the runner is Windows Server), locked-volume contention on a busy host.
