# Spike S4: outside-baseline ratio on public images

Throwaway. Results and interpretation are in `docs/research/baseline-poc.md`.

## What it does
`s4-baseline <$MFT> <vwr.csv>` loads every in-use base FILE record (`mft` @ `18b6c05`, as in S2) and builds paths from parent references with sequence checks. It then looks each file up in a `HashSet` built from the VanillaWindowsReference (VWR) `FullName` column. A `HashSet` has the same set semantics as `fst`; `fst` itself is a Phase 1 H2 decision.

Two modes:
- **raw**: upper-case only.
- **normalized**: also replaces the user-profile name (`%USER%`), SIDs, GUIDs, dotted versions and long hex tokens.

It prints aggregates only: counts, ratios, and the top outside-baseline areas (always shown normalized, so no user names appear). "Last 7 days" is measured back from the newest `$STANDARD_INFORMATION` created time on the volume.

## Inputs (outside the repo, never committed)
| Image | Source | Download MD5 |
|---|---|---|
| Win11 `PC-MUS-001.E01` | Magnet Virtual Summit 2023 CTF | `8cf0c007391f4a72ddc12a570a115b46` |
| Win10 `Microsoft Windows.7z` (`Lenovo ThinkPad.E01`) | Digital Forensics Cookbook datasets (Packt) | `fddc305072321dce24c21fb2a72ff9ee` |

VWR CSVs: `W11_22H2_Pro_20221220_22621.963.csv` and `W10_22H2_Pro_20230321_19045.2728.csv` from `AndrewRathbun/VanillaWindowsReference`.

Extraction uses The Sleuth Kit 4.15 (it reads E01 via libewf):
```sh
mmls PC-MUS-001.E01                      # NTFS partition at sector 239616
icat -o 239616 PC-MUS-001.E01 0  > MFT   # $MFT
icat -o 239616 PC-MUS-001.E01 10 > UpCase
# Win10 image is a bare volume: same commands without -o.
# Build: SOFTWARE hive (ifind -n Windows/System32/config/SOFTWARE, then icat),
# read ProductName / CurrentBuildNumber / UBR / EditionID.
```

## Run
```sh
cargo run --release -- "$TMPDIR/s4/win11/MFT" "$TMPDIR/vwr/W11_22H2_Pro_20221220_22621.963.csv"
```

## Results (2026-10-05, macOS, release build)
| | Win11 | Win10 |
|---|---|---|
| Build (SOFTWARE hive) | 22621.963, Home | 19045.4651, Pro |
| VWR build | 22621.963 Pro (exact) | 19045.2728 Pro (nearest) |
| `$MFT` size / run time | 532 MB / 2.1 s | 79 MB / 1.0 s |
| In-use files with a path (path unknown, corrupt) | 214,262 (0, 0) | 63,350 (0, 0) |
| Created in last 7 days | 21,061 | 63,350 (all) |
| Baseline coverage, normalized | 64.0 % | 11.7 % |
| Outside, all files: raw / normalized | 72.9 % / 70.7 % | 83.1 % / 82.5 % |
| Outside, excluding user profiles (normalized) | 39.2 % | 64.4 % |
| Outside and in last 7 days, % of all files (normalized) | **9.56 %** | n/a (window invalid) |
| `$UpCase` MD5 | `7ff498a44e45e77374cc7c962b1b92f2` | `7ff498a44e45e77374cc7c962b1b92f2` |

Top outside areas, Win11 normalized (% of all files): `USERS\%USER%` 51.9, `PROGRAM FILES\WINDOWSAPPS` 9.0, `PROGRAM FILES\GIT` 2.8, `WINDOWS\WINSXS` 1.7, `PROGRAM FILES\ANDROID` 1.2, `PROGRAMDATA\MICROSOFT` 0.9, `WINDOWS\SYSTEM32` 0.8, `WINDOWS\SERVICING` 0.5.
