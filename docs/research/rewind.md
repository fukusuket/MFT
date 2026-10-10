# Rewind: USN paths at the time of each event

Measured 2026-10-11. P2-3 gate metric: the share of USN events with a full path must be ≥ usnjrnl_rewind's. Own implementation per [ADR 0021](../adr/0021-own-usn-parsing-and-rewind.md); idea from CyberCX (2024), code in `crates/resolve` (`Resolver::rewind`).

## Method

| Step | What |
|---|---|
| Seed | Every `$MFT` record with a name: in use → key `(entry, sequence)`, marked "from the MFT"; deleted → key `(entry, sequence − 1)` (NTFS bumps the sequence when it frees a record), marked "inferred" |
| Walk | `$J` records newest first. For each record: build its parent's path from the map, then set the map entry for the record's own file to the record's name and parent. A record that matches the map changes nothing (and keeps "from the MFT") |
| State | `resolved`: every step from the MFT, unchanged by later records; `inferred`: at least one step learned from the journal or a deleted record; `unknown`: a step is missing or the path would exceed 32,767 UTF-16 units (also ends cycles). No path is ever made up |

Because every record names its file and parent at that moment, `RENAME_OLD_NAME` restores old names and locations, deletes and reused entries (keys include the sequence) need no special case.

Differences from usnjrnl_rewind: it learns only from `RenameOldName` and `FileDelete` records and walks by timestamp; we learn from every record and walk in `$J` order. It reads MFTECmd CSVs; we read the raw files.

## Results

Oracle: usnjrnl_rewind `bc77bcf` on MFTECmd (.NET 9) CSVs, run outside the repo. "Full" = state `resolved` or `inferred` (ours), `ParentPath` without `<UNKNOWN>` (oracle). "Agree" = same parent path, over events both resolve.

| Host | Events | Ours full | of which inferred | Oracle full | Agree |
|---|---|---|---|---|---|
| Simulated-Case-1 dc1 | 607,226 | 607,226 (100 %) | 133,787 | 607,218 | 100 % |
| Simulated-Case-1 desktop6 | 346,981 | 346,981 (100 %) | 45,429 | 346,981 | 100 % |
| Simulated-Case-1 files5 | 345,890 | 345,890 (100 %) | 37,695 | 345,882 | 100 % |
| Yamato ACC-01 | 381,989 | 381,989 (100 %) | 9,995 | 381,989 | 100 % |
| Yamato ACC-03 | 373,016 | 373,016 (100 %) | 9,021 | 373,016 | 100 % |
| Yamato ACC-04 | 368,924 | 368,924 (100 %) | 8,909 | 368,924 | 100 % |
| Yamato ACC-05 | 370,210 | 370,210 (100 %) | 12,202 | 370,210 | 100 % |
| Yamato ACC-09 | 361,513 | 361,513 (100 %) | 10,024 | 361,513 | 100 % |
| Yamato IT-01 | 369,345 | 369,345 (100 %) | 9,876 | 369,345 | 100 % |
| Yamato IT-02 | 376,942 | 376,942 (100 %) | 5,398 | 376,942 | 100 % |
| Yamato IT-03 | 345,169 | 345,169 (100 %) | 8,740 | 345,169 | 100 % |
| Yamato IT-07 | 357,854 | 357,854 (100 %) | 8,166 | 357,854 | 100 % |

- Gate met on all 12 hosts. The oracle's 8 unknown events on dc1 and on files5 are files under a directory that appears in the journal only with other reasons (e.g. `FileCreate`); we resolve them as `inferred` (e.g. `\ProgramData\Microsoft\Diagnosis\Temp\…` on dc1).
- Without Rewind (P2-2, current `$MFT` only) desktop6 had 41,636 `unknown` USN events (12 %).
- Cost on desktop6: 1.0 s, peak RSS 169 MB (74 MB without Rewind; the whole `$J` is held to walk it backwards). dc1: 1.4 s, 245 MB.

## Limits

- A journal that starts mid-history (wrapped or truncated) cannot name directories that were deleted before its first record and are gone from the `$MFT`; those stay `unknown`.
- Deleted `$MFT` records are trusted for `sequence − 1` only; older generations come only from the journal.
