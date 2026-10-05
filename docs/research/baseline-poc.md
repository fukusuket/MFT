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
