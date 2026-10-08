# Baseline PoC (spike S4)

Question: how much of a real volume falls outside the VanillaWindowsReference (VWR) baseline, and is `$UpCase` the same across builds? Code and raw numbers: `spikes/s4-baseline/`.

## Setup
- Images (public, used under their publishers' terms, never committed):
  - Win11 22H2 `PC-MUS-001.E01` (Magnet Virtual Summit 2023 CTF): build 22621.963 Home. VWR has 22621.963 Pro, an exact build match.
  - Win10 22H2 `Lenovo ThinkPad.E01` (Digital Forensics Cookbook datasets): build 19045.4651 Pro. The nearest VWR build is 19045.2728 Pro.
- Builds were read from the SOFTWARE hive.
- Baseline: the VWR `FullName` set. Comparison is case-insensitive, with optional normalization of user name, SID, GUID, version and long-hex tokens.
- Files: in-use, non-directory base records with a resolvable path. NTFS metafiles are excluded.

## Results
| Metric (normalized) | Win11 | Win10 |
|---|---|---|
| Files | 214,262 | 63,350 |
| Outside baseline, all files | 70.7 % | 82.5 % |
| Outside baseline, excluding `Users\<name>` | 39.2 % | 64.4 % |
| Outside baseline **and** created in last 7 days, % of all files | 9.56 % | n/a |
| Files in last 7 days that are outside baseline | 97.3 % (20,483 / 21,061) | n/a |
| `$UpCase` MD5 | `7ff498a4…92f2` | `7ff498a4…92f2` |

Path resolution: no corrupt records and no unresolved paths on either image. A 532 MB `$MFT` takes 2.1 s with a `HashSet` lookup.

**Correction (P1-3b, 2026-10-07).** The spike ignored extension records, which hold the Win32 long names of some files and directories. With them merged into their base records, the real code measures Win11 at 214,385 files and 69.1 % outside (148,195): 2,766 paths now match the baseline. Win10 is unchanged.

## Findings
1. **`$UpCase` is identical** on Win10 22H2 and Win11 22H2 and matches ADR 0009. No supersede needed.
2. **The whole-volume ratio is far above 10 %.** Where the outside files are (Win11):
   - User profiles: about half of all files.
   - Installed apps: WindowsApps 9 %, Git, Android SDK, others.
   - Post-baseline updates: WinSxS, System32, servicing.

   A clean-install baseline cannot cover user data or apps.
3. **The time window does the reduction, not the baseline.**
   - Outside-baseline files created in the window are 9.56 % of all files on Win11. That meets the spike target (< 10 %) but not the product target in `product.md` §4 (< 5 %).
   - Inside the window, 97 % of files are outside the baseline. The baseline mainly labels files as `standard` vs `outside`; it removes little once the window is applied.
4. **Normalization helps little**: 72.9 % raw vs 70.7 % normalized on Win11, mostly WinSxS version folders. User-name normalization matters for matching `Users\Default`-derived files, not for the overall ratio.
5. **Edition mismatch** (Home image vs Pro VWR) was not separated from other causes.
6. **The Win10 image is not representative.**
   - Every `$SI` created time falls within 7 days, probably reset when the image was prepared, so the window metric is meaningless there.
   - Only 11.7 % of the baseline is present (System32 is largely missing).
   - It confirms the build and the `$UpCase` result only.

## Implications for Phase 1 (suggestions, not decided)
- Measure the product metric in the window (as `product.md` §4 defines it), not over the whole volume.
- The < 5 % product target looks unreachable with a clean-install baseline alone. Options:
  - Report outside-baseline files grouped by write privilege, which `product.md` §6 already plans.
  - Revisit the target at the Phase 1 gate with the clean Win11 24H2 VM.
- Keep the normalization set small (user, SID, GUID, version) until the VM comparison shows a need for more.

## Phase 1 gate (P1-6)

### Scale on the Mac (2026-10-08)
- Input: a synthetic 1 GiB `$MFT` (1,048,576 records of 1024 bytes) from `cargo run -p xtask --release -- synth-mft 1048576 <out>`. It is built with the `mft-parse` test builder; the same count always gives the same bytes. Shape:
  - 1,038,086 entries after merging extension records; paths mostly 8–9 levels deep, at most 19;
  - about 5 % directories and 5 % deleted entries;
  - 1 % long names held in extension records, and 1,048 BAAD records;
  - files planted so every sample rule fires.
- Run: release build, `tool analyze` with the W11 22H2 baseline, `testdata/rules`, `-o` and `--csv`, on an Apple Silicon Mac. Measured with `/usr/bin/time -l`, two runs each.

| Measure | First build | After streaming the HTML data | Gate |
|---|---|---|---|
| Wall time | 9.15 s / 9.13 s | 8.83 s / 8.88 s | ≤ 60 s |
| Maximum resident set size | 2,179,694,592 / 2,188,165,120 B | 1,019,133,952 / 1,045,905,408 B | ≤ 2 GiB |
| Outputs of the two runs | byte-identical | byte-identical | — |

- The first build failed the RAM limit by about 2 %. It held the whole report JSON (about 500 MB) and its Base64 copy (667 MB) at once.
- The fix streams JSON → Base64 → file. Output is byte-identical to the first build's.
- The synthetic volume is a worst case for the HTML: 984,603 files (95 %) are outside the baseline and 341,140 entries have findings, so the report is 667 MB. Real volumes are smaller, and smaller again once the time window is applied.
- CI runs the determinism check on a 20,000-record synthetic volume with every output enabled (`synthetic_volume_gives_byte_identical_reports`).

### Windows VM
Pending: outside-baseline ratio in the window, VWR gaps after a clean install, and `$UpCase` MD5 on Win11 24H2 x64.
