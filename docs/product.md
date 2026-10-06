# Product

What we build and why. How: [architecture.md](architecture.md). When: [plan.md](plan.md).

Working name: `tool` (final name TBD). Inputs: `$MFT` and `$UsnJrnl:$J` only.

## 1. Problem

Find what an investigator should look at on a Windows host **even when event logs were wiped**, using only NTFS metadata, in tens of minutes, without forensic expertise.

| # | Problem today | How we solve it |
|---|---|---|
| P1 | Event logs are routinely deleted; log-based tools (Hayabusa) then see nothing | Use only `$MFT` / `$J`. Detect the log deletion itself (`.evtx` deletes in USN) |
| P2 | Full forensics is slow (days to image, transfer, process) | Collect only `$MFT`, `$J` (sparse part stripped), `$Boot`, `$Secure:$SDS`: tens to hundreds of MB |
| P3 | Commercial tools are expensive | OSS, single binary, no install |
| P4 | MFT parsers output CSV with no guidance | Show only rule matches, each with the matched condition, evidence values and a plain-language explanation |
| P5 | Normal Windows activity drowns the signal | Subtract a Windows baseline and filter by time window |
| P6 | Results are hard to share | One self-contained HTML report, opens offline in any browser |
| P7 | IoCs/signatures miss unknown tools | Surface "not in the Windows baseline" regardless of known-bad patterns |

Out of reach from NTFS metadata (stated in the UI): who or which process created/ran a file, file content (except resident data), whether a file was executed, activity older than the USN journal.

## 2. Target users

| | Persona | Need |
|---|---|---|
| **Primary** | IT admin at a small/mid company, not a security specialist, first responder to ransomware or an EDR alert | Collect from USB, open a report, understand the next step without NTFS knowledge |
| Secondary | In-house CSIRT/SOC (non-vendor) | Consistent rules and baselines across hosts; merge with Hayabusa timelines |
| Secondary | Security staff with little budget (public sector, education, healthcare, small MSPs) | Free; subtract their own golden image |
| Tertiary | DFIR professionals | Drill down to raw records; CSV/JSONL export |

Design priority: the primary user must never get stuck. Expert detail goes into detail views and exports.

## 3. Differentiation

| | **tool** | MFTECmd / `mft` crate | usnjrnl-forensic | Hayabusa | Commercial suites |
|---|---|---|---|---|---|
| Purpose | NTFS triage: what to look at | Parsing | Parse + correlate + report | Event log triage | Full forensics |
| **Windows baseline diff** | **Core** | No | No | n/a | Hash sets only |
| Time-window focus | Yes | No | Yes | Yes | Yes |
| Rules | Sigma (SigmaHQ + own) | No | Own | Sigma | Proprietary |
| Works after log wipe | Yes | Yes | Yes | No | Yes |
| Explains matches | Yes | No | Partly | Yes | Partly |
| Status | New | Active | Archived 2026-07 | Active | Paid |

Relationship to Hayabusa: complementary. Hayabusa extracts events from logs; this tool extracts files and file activity from the filesystem. Output columns are aligned so timelines can be merged.

## 4. MVP

**Goal:** the primary user collects from one Windows 10/11 client and, within 30 minutes, opens an HTML report and knows what to check first.

| # | In scope |
|---|---|
| M1 | `collect`: single Windows x64 exe; reads the raw volume; writes `$MFT`, `$J`, `$Boot`, `$SDS`, `meta.json` into one zip |
| M2 | `analyze` also accepts files collected by KAPE, Velociraptor or FTK Imager |
| M3 | Parse `$MFT` and USN V2/V3/V4 |
| M4 | Path resolution with sequence checks and Rewind; each path is `resolved` / `inferred` / `unknown` |
| M5 | Baseline diff for Win10 22H2 and Win11 24H2/25H2 x64 (en, ja); nearest-build fallback |
| M6 | Sigma detection incl. `event_count` and `temporal_ordered` correlations |
| M7 | `--from/--to` filters the view (detection always runs on everything); warn when the window is not covered by the USN journal |
| M8 | Self-contained HTML: Summary, Findings, Outside-baseline files, Timeline, File detail (en/ja) |
| M9 | `findings.jsonl` and `timeline.csv` compatible with Hayabusa/Takajo timelines |

Out of scope for MVP: Windows Server baselines, custom baselines, multi-host comparison, YARA, WASM, server mode, `$LogFile`, non-NTFS.

| Success criterion | Target |
|---|---|
| Collection to report, one client | ≤ 30 min |
| Collected size (compressed) | ≤ 100 MB |
| Analysis of 1 GB `$MFT` + 32 MB `$J` | ≤ 1 min, ≤ 2 GB RAM |
| Outside-baseline entries in the window | < 5 % of all entries |
| Attack-related files shown (Findings or Outside-baseline) on the attack corpus | ≥ 80 % |
| Non-experts reach the report using only the manual | Usability test, ≥ 3 people |

## 5. CLI

```
tool collect   [-v C:] [-o collection.zip]
tool analyze   -i <collection.zip | dir> [--from DATE] [--to DATE] [--tz +09:00]
               [--baseline auto|<name>] [--lang en|ja] [--mask-users]
               [-o report.html] [--jsonl findings.jsonl] [--csv timeline.csv]
tool baseline  list | update
tool rules     list | update
```

```
$ tool analyze -i PC-042.zip --from 2026-09-28 --to 2026-10-02 -o PC-042.html
[1/6] Ingest     $MFT 626,688 records (3 corrupt, skipped)  $J 1,203,551 records
[2/6] Resolve    resolved 99.2% / inferred 0.6% / unknown 0.2%
[3/6] Baseline   Win11 24H2 26100.4061 -> nearest 26100.3915
                 outside baseline 9,412 (1.5%), 418 in window
[4/6] Detect     214 rules, 41 files matched
[5/6] Window     2026-09-28 .. 2026-10-02 (+09:00)
                 USN coverage 2026-09-14 .. 2026-10-03 (window covered)
[6/6] Report     PC-042.html (7.9 MB)

critical  Event Log File Deleted          2026-10-01 02:13  C:\Windows\System32\winevt\Logs\Security.evtx
high      System Binary Outside System32  2026-09-30 23:43  C:\ProgramData\svchost.exe
...
```

If the window is not fully covered by USN, warn: "No USN data before 2026-09-25; absence of findings in that range does not mean nothing happened."

## 6. UI

| Screen | Content |
|---|---|
| Summary | Host, OS build, window, USN coverage, baseline used; labeled counts (critical, high, outside-baseline in window, deleted `.evtx`); key events; activity sparkline; "what this report cannot tell you" |
| Findings | Level → time; filters by level, rule, window, facts |
| Outside-baseline files | All outside-baseline files in the window, grouped by privilege needed to write there (admin/SYSTEM, any user, user profile) |
| Timeline | USN events and MFT timestamps for findings and outside-baseline files |
| File detail | Matched rules with condition and evidence; explanation, limits, recommended checks; USN lifecycle; `$SI`/`$FN` timestamps; raw record reference |

```
C:\ProgramData\svchost.exe
Outside baseline · path: resolved · seen in MFT and USN · owner S-1-5-32-544
 high  System Binary Outside System32   TargetFilename|endswith '\svchost.exe' and not under System32/SysWOW64
 Why    svchost.exe normally exists only in System32; placing a system name elsewhere is a common masquerade.
 Limits NTFS metadata cannot show content, creating process, or execution.
 Check  Collect the file and hash it; check Prefetch, services, EDR telemetry.
 USN    23:43:18 FILE_CREATE · 23:43:20 BASIC_INFO_CHANGE
 Times  $SI created 2021-06-05 12:10   $FN created 2026-09-30 23:43   (SI<FN, zero sub-seconds)
 Raw    $MFT entry 184,203 seq 4 @0x0B3E2C00
```

Severity is shown as text, not color alone. Filter state is kept in the URL hash.

## 7. Open questions

| # | Question | Current leaning |
|---|---|---|
| 2 | Baseline granularity | Per build, nearest UBR fallback; normalize language differences |
| 3 | Rule level criteria | Based on the privilege needed to write the path; document in `docs/rule-levels.md` |
| 4 | Collector code signing | Sign from MVP |
| 5 | Report size limit for file servers | MVP targets clients; spill out-of-window data to CSV |
| 6 | Tool name | — |
