# Architecture

How the tool is built. Decisions behind it: [adr/](adr/). Library choices: [research/oss-reuse.md](research/oss-reuse.md).

## Principles
- One Rust core; the CLI does the heavy work; the browser only views pre-filtered results.
- **No computed scores.** A finding is the list of matched Sigma rules with their fixed `level`; order is level → time.
- **Detection logic lives in rule files**, not in Rust. The core only exposes facts.
- Same input → byte-identical output.
- No network access.
- Baselines and rules are versioned data, released separately from the binary.

## Pipeline

Today `mozu analyze` reads a raw `$MFT` and an optional raw `$J`:

```
--rules dirs ─► load rules (skip what cannot match)        --baseline .fst ─► load baseline
$MFT ─► parse records ─► merge extension records ─► entries
$J   ─► parse records (V2/V3; V4 reported) ─────────► USN records
entries + USN ─► resolve (paths, Rewind) ─► baseline lookup, facts, folder, window
              ─► detect (Sigma) ─► rows: every $MFT entry, then every USN record in $J order
rows ─► CSV / JSONL / HTML (+ skipped-rules lists), each written atomically
```

Target (Phase 3): collection and collection-directory/zip input.

```
[collect] raw volume → $MFT, $J, $Boot, $SDS, meta.json → collection.zip
                                     │  (or KAPE / Velociraptor output)
                                     ▼
[analyze] ingest+validate → parse → resolve (Rewind) → normalize
          → baseline lookup → facts → detect (Sigma)  ◄── baseline DB, rules
                                     │
                                     ▼
          report.html (data embedded) + findings.jsonl + timeline.csv
```

## Baseline and Sigma

```
all USN events + all MFT entries
  ├─► Detect A: Sigma, logsource file_event / file_delete / file_rename / file_change
  ├─► Baseline lookup ─► outside-baseline set
  │       ├─► Detect B: Sigma, logsource service: baseline_outside
  │       └─► "Outside-baseline files" list (shown even without a rule match)
  └─► Findings = matches of A + B (listed, never summed)
```

Supported Sigma file-event rules can be used unchanged when their logsource, fields and modifiers are among those emitted by this tool. The standard fields are `TargetFilename`, `SourceFilename`, `CreationUtcTime` and `UtcTime`; `SiCreatedBeforeFnCreated` is tool-specific. "Outside baseline" is a logsource service, not a custom field. Correlation and filter rules are not supported yet.

A rule loads only if it can match what `detect` emits (`detect::EMITTED`). Otherwise it is skipped with one of these reasons; only an id used twice or an I/O error stops the run.

| Reason | When |
|---|---|
| `parse: …` | The parser rejects the document (not UTF-8, invalid YAML, no detection, unknown modifier) |
| `symbolic link not followed` | A link under a rules directory; links are listed, never followed |
| `correlation rule`, `filter rule` | Not supported yet |
| `no id` | Findings link to rules by id (ADR 0003) |
| `logsource not emitted: …` | `product`, `category` or `service` is set to a value never emitted; an unset one matches anything |
| `field not emitted: …` | The detection reads a field never emitted (e.g. `Image`); keyword detections read none |
| `engine: …` | The engine cannot compile the rule (e.g. an invalid regex) |

USN events come from the record with `CLOSE`, which carries every reason of its handle, so one operation is one event. Directories give none.

| Reason on the closing record | Category | Fields |
|---|---|---|
| `FILE_CREATE` | `file_event` | `TargetFilename`, `CreationUtcTime` and `UtcTime` (USN time) |
| `RENAME_NEW_NAME` | `file_rename` | `SourceFilename` (from the same file's last `RENAME_OLD_NAME`, when its path is known), `TargetFilename`, `UtcTime` |
| `BASIC_INFO_CHANGE` | `file_change` | `TargetFilename`, `UtcTime` |
| `FILE_DELETE` | `file_delete` | `TargetFilename`, `UtcTime` |

Data writes (`DATA_*`) are not events. An operation still open at the end of `$J` has no closing record and gives no event.
A rule without `category` matches each event of a record, so it can list the same finding twice. USN baseline status uses the path whenever it is known, `inferred` included: those names come from the journal, not guesses.

Facts shown next to findings, never used to rank them: baseline status, path-resolution state (`resolved`/`inferred`/`unknown`), and:

| Fact | `$MFT` row | USN row |
|---|---|---|
| `deleted` | Not in use | The `$MFT` has no in-use entry for the event's file |
| `seen` | `both` if `$J` has an event for its file, else `mft` | `both` if the `$MFT` has an entry for that file, else `usn` |
| `streams` | Names of named `$DATA` attributes (ADS, `Zone.Identifier` included) | — |
| `hints` | `si_before_fn` (`$SI` created < shown name's `$FN` created), `si_zero_subseconds` (`$SI` created) | — |

An entry's file is its entry and sequence number; for a deleted entry, the sequence number before the delete (NTFS bumps it on free, as Rewind assumes). `streams` and `hints` come from the `$MFT` and are empty on USN rows. Owner SID (`$SDS`) comes later.

## Folder

Each row's `folder` says where the file sits, judged by the directories above it (not its own name), comparing whole names case-insensitively (ASCII). The first match wins:

| `folder` | Directories start with |
|---|---|
| `public` | `Users\Public` |
| `downloads`, `desktop`, `documents` | `Users\<name>\` that folder |
| `temp` | any directory named `Temp` |
| `appdata` | `Users\<name>\AppData` |
| `user_profile` | `Users\<name>` |
| `programdata`, `program_files`, `windows` | `ProgramData`; `Program Files` or `Program Files (x86)`; `Windows` |
| `other` | anything else with a known path |
| `no_path` | unknown path (USN: unknown directory) |

## Time window

`--from`/`--to`/`--tz` give a window `[start, end)` in UTC. A date-only `--to` ends at the end of that day; `--to` with a time, at the end of that second. Detection runs on every row; the window only selects what the HTML lists and counts, and CSV/JSONL mark each row `in_window` (empty without a window).

| Row | In the window when |
|---|---|
| `$MFT` | `$SI` created or the shown name's `$FN` created is in it (ADR 0011); `false` with neither |
| USN event | its USN time is in it |
| USN diagnostic | never (`false`) |

The report warns when the window starts before the earliest USN event (or has no start): findings cannot cover that part. Without `$J` there is no USN coverage to compare, so no warning.

## Outputs

At least one of `-o`, `--csv`, `--jsonl` is required. Each file is written to a temporary name and renamed when complete; the skipped-rules lists are committed first, so a report never exists without them. An output that is the same file as an input or another output is refused.

| Output | Content |
|---|---|
| `--csv` | One row per `$MFT` entry, then one per USN record. Columns: `entry`, `sequence`, `in_use`, `name`, `path`, `path_state`, `baseline`, `findings`, `si_created`, `fn_created`, `diagnostics`, `source` (`mft`/`usn`), `usn`, `reasons`, `event_time`, `deleted`, `seen`, `streams`, `hints`, `in_window`, `folder`. Cells are formula-neutralized |
| `--jsonl` | The same rows and fields, typed; `findings` is a list of `{level, id}` |
| `-o` | One HTML file, data embedded, no external resources (CSP `default-src 'none'`). Tabs Summary, Findings, Outside-baseline; filters and tab kept in the URL hash. Lists only rows in the window. Summary shows the `Edition` (from `\Windows\System32\spp\tokens\skus\`) and warns when the baseline is of another edition |
| `<out>.skipped-rules.csv` / `.jsonl` | Rules not loaded and why (see the table above); written beside the CSV/JSONL, or beside the HTML when it is the only output, whenever `--rules` is given |

All times are UTC (ISO 8601). Paths and names are attacker input: control and bidi characters are shown as `\u{XX}` in every output; HTML is built without `innerHTML`.

### Output values

| Field | Values and meaning |
|---|---|
| `path_state` | `resolved`: built from current records; `inferred`: built partly by Rewind from `$J`; `unknown`: no path is invented |
| `baseline` | `standard` / `outside`; empty in CSV and `null` in JSONL when no path or baseline result exists |
| `findings` | CSV: `level:id` items separated by `;`; JSONL: `{level, id}` objects, most severe first |
| `diagnostics` | Parser codes separated by `;` in CSV and an array in JSONL. A USN diagnostic row has unavailable record fields empty/`null` |
| `reasons`, `streams`, `hints` | Named values separated by `;` in CSV and arrays in JSONL |
| `seen` | Whether the file reference was observed in `mft`, `usn` or `both`; unavailable on diagnostic rows |
| `in_window` | `true` / `false` with a window; empty in CSV and `null` in JSONL when no window was requested |

CSV follows RFC-style quoting through the Rust `csv` crate. Evidence cells beginning with `=`, `+`, `-` or `@` are prefixed with `'` to prevent spreadsheet formula execution.

## Crates

All crates below are implemented. `report`'s viewer (`viewer.js`) is plain JS tested under Node (ADR 0020, ADR 0022); `resolve`'s Rewind is compared in [research/rewind.md](research/rewind.md). `fuzz/` is a separate workspace (ADR 0013).

| Crate | Role | Depends on | Built on |
|---|---|---|---|
| `ntfs-types` | `NtfsName`, `NormPath`, `Filetime`, `FileRef`. No I/O, no logic | — | — |
| `mft-parse` | `$MFT` → `Entry` + `Diagnostic` | ntfs-types | `mft` @ `18b6c05`, no default features (ADR 0004) |
| `usn-parse` | `$J` → `UsnEvent` (V2/V3; V4 reported) | ntfs-types | own code (ADR 0021) |
| `resolve` | MFT↔USN join, Rewind | mft-parse, usn-parse | own code (ADR 0021) |
| `baseline` | Normalize, build and query an `fst` | ntfs-types | `fst`, `csv` |
| `sigma` | Thin adapter over the Sigma engine; NTFS-agnostic | — | `rsigma-eval` `=0.23.0`, no default features (ADR 0003) |
| `detect` | NTFS events → Sigma events, logsource mapping | sigma, resolve, baseline, mft-parse, usn-parse | — |
| `analyze` | Pipeline, facts, findings, sorting | detect, sigma, baseline, resolve, mft-parse, ntfs-types, usn-parse | — |
| `report` | HTML, JSONL, CSV | analyze, baseline, mft-parse, ntfs-types, resolve, sigma, usn-parse | `csv`, `serde`, `serde_json`; viewer in plain JS (ADR 0020) |
| `cli` | `analyze`, `baseline build`; time-window parsing, atomic outputs | analyze, baseline, detect, mft-parse, ntfs-types, report, sigma, usn-parse | `clap`, `anyhow` |
| `xtask` | Deterministic synthetic `$MFT` for gate measurements | — | — |

Phase 3 adds `collector`, built on Windows with `ntfs-reader =0.6.0` (ADR 0005), and the `collect`, `baseline update`, and `rules` CLI surfaces.

Repository plan: this one (code, AGPL-3.0); `mozu-rules` (Sigma rules + `explain/{en,ja}/`, DRL 1.1); `mozu-baselines` (fst files + generation CI). The current implementation in this repository does not consume the latter two yet.
