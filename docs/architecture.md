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

Rules use only standard Sigma fields (`TargetFilename`, `SourceFilename`, `CreationUtcTime`), so SigmaHQ rules work unchanged. "Outside baseline" is a logsource, not a custom field.

Facts shown next to findings, never used to rank them: baseline status, timestomp hints (`$SI < $FN`, zero sub-seconds), Zone.Identifier, ADS, deleted, path-resolution state (`resolved`/`inferred`/`unknown`), seen in MFT/USN/both, owner SID.

## Crates

So far: `ntfs-types`, `mft-parse`, `usn-parse`, `resolve` (MFT paths and USN Rewind, [research](research/rewind.md)), `baseline` (no zstd yet), `sigma`, `detect`, `analyze`, `report` (CSV, JSONL timeline and a static HTML template; viewer per ADR 0020 not built yet), `cli`, `xtask` (synthetic `$MFT` for the gate). `fuzz/` is a separate workspace (ADR 0013).

| Crate | Role | Depends on | Built on |
|---|---|---|---|
| `ntfs-types` | `NtfsName`, `NormPath`, `Filetime`, `FileRef`. No I/O, no logic | — | — |
| `mft-parse` | `$MFT` → `Entry` + `Diagnostic` | ntfs-types | `mft` @ `18b6c05`, no default features (ADR 0004) |
| `usn-parse` | `$J` → `UsnEvent` (V2/V3; V4 reported) | ntfs-types | own code (ADR 0021) |
| `resolve` | MFT↔USN join, Rewind | mft-parse, usn-parse | own code (ADR 0021) |
| `baseline` | Normalize, `fst` lookup, manifest | ntfs-types | `fst`, `zstd` |
| `sigma` | Thin adapter over the Sigma engine; NTFS-agnostic | — | `rsigma-eval` `=0.23.0`, no default features (ADR 0003) |
| `detect` | NTFS events → Sigma events, logsource mapping | sigma, resolve, baseline, mft-parse | — |
| `analyze` | Pipeline, facts, findings, sorting | detect, sigma, baseline, resolve | `rayon`, `indexmap` |
| `report` | HTML, JSONL, CSV | analyze (and the types it returns: sigma, baseline, resolve) | `csv`; viewer in plain JS (ADR 0020) |
| `collector` | Raw volume read (Windows) | ntfs-types | `ntfs-reader =0.6.0`, Windows only (ADR 0005) |
| `cli` | `collect`, `analyze`, `baseline`, `rules` | all | `clap` |

Repositories: this one (code, AGPL-3.0); `tool-rules` (Sigma rules + `explain/{en,ja}/`, DRL 1.1); `tool-baselines` (fst files + generation CI).
