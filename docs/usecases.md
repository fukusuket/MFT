# Use cases

Recipes for the current CLI. Setup and options: [README](../README.md#quick-start). Users: [product.md §2](product.md#2-target-users). Output columns: [architecture.md](architecture.md#outputs).

| # | I want to… | Section |
|---|---|---|
| U1 | See what happened after a ransomware or EDR alert | [First look](#u1-first-look-after-an-incident) |
| U2 | Know whether event logs or Prefetch were deleted | [Log deletion](#u2-log-or-trace-deletion) |
| U3 | List what Windows does not ship | [Outside the baseline](#u3-outside-the-baseline) |
| U4 | Run the same check on many hosts | [Many hosts](#u4-many-hosts) |
| U5 | Use the rows in `jq` or Timeline Explorer | [Own tools](#u5-own-tools-jq-timeline-explorer) |
| U6 | Add my own or SigmaHQ rules | [Own rules](#u6-own-rules) |
| U7 | Use my golden image as the baseline | [Own baseline](#u7-own-baseline) |

## U1 First look after an incident

1. **Collect** the raw `$MFT` and `$UsnJrnl:$J` with KAPE, Velociraptor or FTK Imager. Extract them as plain files (zip and collection directories are not read yet).
2. **Baseline:** use one of the same edition (client vs Server). The Summary warns on a mismatch.
3. **Run** with a window around the alert, in the host's UTC offset:
   ```sh
   mozu analyze -i '$MFT' --usn '$J' --baseline win11-24h2.fst --rules testdata/rules \
     --from 2026-09-28 --to 2026-10-02 --tz +09:00 -o PC-042.html
   ```
4. **Open** `PC-042.html` → Summary → click the critical/high count → Findings.
5. **Read** each finding's facts (`deleted`, ADS, `seen`, hints). Select the row to see why it matched.
6. **Check the window warning.** "No USN data before X" means findings cannot exist there, not that nothing happened.

**Next:** NTFS metadata cannot say who created a file, whether it ran, or what it contains. Collect the file; check EDR, Prefetch and services.

## U2 Log or trace deletion

Run with `--usn`. Deleted `C:\Windows\System32\winevt\Logs\*.evtx` and Prefetch files become findings (`event_log_deleted.yml`, `prefetch_deleted.yml`). Omit `--from`/`--to` to see the whole journal.

## U3 Outside the baseline

Open the **Outside-baseline** tab: files not in the baseline, counted per folder (`public`, `downloads`, `temp`, `programdata`, …). Click a folder, then narrow by time or path. Files show here even without a rule match, so unknown tools are not missed.

## U4 Many hosts

Same `--baseline` and `--rules` per host. Same input → same bytes, so reports can be diffed and archived.

```sh
for h in hosts/*/; do
  mozu analyze -i "$h/\$MFT" --usn "$h/\$J" --baseline win11-24h2.fst --rules rules/ \
    --jsonl "out/$(basename "$h").jsonl"
done
```

## U5 Own tools (jq, Timeline Explorer)

`--csv` and `--jsonl` hold every row; `in_window` marks the window.

```sh
# Findings: level, time, path
jq -c 'select(.findings != []) | [.findings[0].level, .event_time // .si_created, .path]' timeline.jsonl
# Deleted files outside the baseline
jq -c 'select(.deleted and .baseline == "outside") | .path' timeline.jsonl
```

The CSV opens in Timeline Explorer or a spreadsheet; formula cells are neutralized.

## U6 Own rules

Pass `--rules` once per directory (`.yml`/`.yaml`, subdirectories included).

```sh
mozu analyze -i '$MFT' --usn '$J' --rules testdata/rules --rules ~/sigma/rules/windows/file -o report.html
```

| Supported | Values |
|---|---|
| Fields | `TargetFilename`, `SourceFilename`, `CreationUtcTime`, `UtcTime` |
| Logsources | `file_event`, `file_rename`, `file_change`, `file_delete`, `service: baseline_outside` |

Rules that cannot match are skipped and listed with the reason in `*.skipped-rules.csv` / `.jsonl` ([reasons](architecture.md#baseline-and-sigma)).

## U7 Own baseline

Export a clean install of the same edition with VanillaWindowsReference. The CSV needs a `FullName` column; only `C:\` entries are used.

```sh
mozu baseline build --vwr golden-image.csv -o golden.fst
mozu analyze -i '$MFT' --baseline golden.fst -o report.html
```

Keep in mind:
- The `.fst` stores no edition or build. The report shows its filename; the edition warning is a folder-based check, not a build match.
- A VWR export can miss files. "Outside baseline" is a lead, not evidence of compromise.

Ready-made baselines and `--baseline auto` come in Phase 4.

## Not covered

- From NTFS metadata: creating user or process, execution, file content, activity older than the USN journal ([product.md §1](product.md#1-problem)).
- Not built yet: zip/collection input, `mozu collect`, Timeline and File detail screens, correlation rules, en/ja ([plan.md](plan.md) Phases 3–4).
