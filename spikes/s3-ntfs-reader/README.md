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

## Results
Pending the runner job.
