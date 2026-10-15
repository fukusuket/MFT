# Use cases

How the tool is used, step by step. Users and needs: [product.md §2](product.md#2-target-users). Outputs: [architecture.md](architecture.md#outputs).

Status: **today** works with the current CLI; **Phase 3** needs an item in [plan.md](plan.md).

| # | Use case | User | Status |
|---|---|---|---|
| U1 | Ransomware or EDR alert: what happened on this host? | IT admin (primary) | today, with files copied by KAPE/Velociraptor/FTK Imager; `tool collect` in Phase 3 |
| U2 | Were event logs or Prefetch deleted? | IT admin, CSIRT | today |
| U3 | What is new on this host that Windows does not ship? | IT admin, CSIRT | today |
| U4 | Same rules and baseline across many hosts | CSIRT/SOC | today (one run per host) |
| U5 | Raw records into my own tools | DFIR | today |
| U6 | Own rules or SigmaHQ rules | CSIRT, DFIR | today |
| U7 | Own golden image as the baseline | Low-budget security staff | today, from a VanillaWindowsReference CSV |

## U1 First look after an incident

1. Get `$MFT` and `$J` from the host (KAPE, Velociraptor or FTK Imager today; `tool collect` in Phase 3).
2. Build or pick a baseline of the **same edition** (client vs Server); the Summary warns on a mismatch.
3. Run with a window around the alert, in the host's local offset:
   ```sh
   tool analyze -i '$MFT' --usn '$J' --baseline win11-24h2.fst --rules testdata/rules \
     --from 2026-09-28 --to 2026-10-02 --tz +09:00 -o PC-042.html
   ```
4. Open `PC-042.html`. Summary → click the critical/high count → Findings, ordered level → time.
5. For each finding read the facts line (`deleted`, ADS, `seen`, hints); select the row to see why it matched.
6. Check the Summary window warning: no USN data before a date means no findings can exist there, not that nothing happened.

Next steps lie outside NTFS metadata (who created a file, whether it ran, its content): collect the file, check EDR, Prefetch, services.

## U2 Log or trace deletion

Event logs wiped → log-based tools see nothing. With `--usn`, `FILE_DELETE` of `C:\Windows\System32\winevt\Logs\*.evtx` and of Prefetch files are findings (sample rules `event_log_deleted.yml`, `prefetch_deleted.yml`). Run without `--from/--to` to see the whole journal.

## U3 Outside the baseline

The Outside-baseline tab lists files whose path is not in the baseline, with counts per folder (`public`, `downloads`, `temp`, `programdata`, …). Click a folder, then narrow by time or path. Files appear here even when no rule matched: unknown tools have no signature.

## U4 Many hosts

Run the same command per host with the same `--baseline` and `--rules`; output is byte-identical for the same input, so reports can be diffed and archived.

```sh
for h in hosts/*/; do
  tool analyze -i "$h/\$MFT" --usn "$h/\$J" --baseline win11-24h2.fst --rules rules/ \
    --jsonl "out/$(basename "$h").jsonl"
done
```

## U5 Own tools (jq, Timeline Explorer)

`--csv` and `--jsonl` hold every row, in or out of the window; `in_window` marks the window.

```sh
# Findings only: top level, time, path
jq -c 'select(.findings != []) | [.findings[0].level, .event_time // .si_created, .path]' timeline.jsonl
# Deleted files outside the baseline
jq -c 'select(.deleted and .baseline == "outside") | .path' timeline.jsonl
```

The CSV opens in Timeline Explorer or a spreadsheet (formula cells are neutralized).

## U6 Own rules

`--rules` takes any number of directories of `.yml`/`.yaml` Sigma rules, subdirectories included. Use the standard fields `TargetFilename`, `SourceFilename`, `CreationUtcTime`, `UtcTime`; logsources `file_event`, `file_rename`, `file_change`, `file_delete`, or `service: baseline_outside`. Rules that cannot match are skipped and listed in `*.skipped-rules.csv` / `.jsonl` with the reason ([architecture.md](architecture.md#baseline-and-sigma)).

```sh
tool analyze -i '$MFT' --usn '$J' --rules testdata/rules --rules ~/sigma/rules/windows/file -o report.html
```

## U7 Own baseline

Export a clean image with VanillaWindowsReference, then:

```sh
tool baseline build --vwr golden-image.csv -o golden.fst
tool analyze -i '$MFT' --baseline golden.fst -o report.html
```

Ready-made baselines per build and `--baseline auto` come in Phase 3.

## Not covered

From NTFS metadata alone: the creating user or process, whether a file was executed, file content, activity older than the USN journal ([product.md §1](product.md#1-problem)). Not built yet: zip/collection input, `tool collect`, Timeline and File detail screens, correlation rules, en/ja ([plan.md](plan.md) Phase 3).
