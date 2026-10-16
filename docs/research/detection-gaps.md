# Detection rule gaps (curated)

Surveyed 2026-10-17. Goal: close real gaps with the **fewest** rules. Most apparent gaps are tag gaps, other-layer problems or noise; those get no new rule.

## Inputs

| Source | Version | Count used |
|---|---|---|
| ATT&CK Enterprise (`mitre/cti`) | v20.0 (2026-10-13) | 479 active Windows techniques; 1,770 analytics |
| SigmaHQ `rules`, `rules-threat-hunting`, `rules-emerging-threats` | 2026-10-14 | 3,757 |
| hayabusa-rules `hayabusa/` (native only; `sigma/` is a conversion) | 2026-10-14 | 196 |
| Yara-Rules `rules` | last commit 2022-04-12 | ~13,000 rules |

Coverage = an `attack.tXXXX` tag on a Windows rule. "Strong" = `stable`/`test`, `high`/`critical`, not threat-hunting. "Uses" = ATT&CK `uses` links from groups, software and campaigns (a reporting-biased proxy for prevalence).

## Baseline

| Metric | Value |
|---|---|
| Windows techniques with any rule | 319 / 479 (67%) |
| ... with a strong rule | 240 / 479 (50%) |
| Windows rules on `process_creation` | 1,418 / 3,068 (46%) |
| Sigma `high`/`critical` with FP "Unknown" and no `regression_data` | 853 / 1,491 |
| Correlation rules (Sigma / Hayabusa) | 0 / 3 |

## Gate for a new rule

A new rule is written only if **all** hold:

| # | Condition |
|---|---|
| G1 | Technique has ≥ 50 uses, or its impact is destructive |
| G2 | No existing rule detects it after re-tagging (check by logic, not by tag) |
| G3 | Windows logs carry a signal that separates it from routine admin; `high` needs 0 hits on a clean baseline |
| G4 | A regression sample can be produced (Atomic Red Team, EVTX-ATTACK-SAMPLES) |
| G5 | Not better served by correlation over existing rules or by another layer (YARA, NSM, EDR) |

## Decisions per candidate

| Candidate (uses) | Apparent gap | Decision | Reason |
|---|---|---|---|
| Discovery: T1082 (437), T1057 (326), T1016 (292), T1083 (381), T1680 (103) | Strong 6/1/1/4/0 | **Correlate** (C1) | Single commands are admin noise (G3); rules already exist |
| T1680 Local Storage Discovery | 0 rules | **Re-tag** | `proc_creation_win_fsutil_drive_enumeration` (tagged T1120) and `proc_creation_win_wmic_recon_volume` (T1082) already detect it |
| T1110.003 / .001 password spray, guessing | Sigma has none | **Correlate** (C2) | Port Hayabusa's 3 correlation rules; no new logic |
| T1070.006 Timestomp (59) | Strong 0 | **This tool, later** | Sysmon 2 is noisy (Hayabusa keeps it `low`). `$SI` < `$FN` is an NTFS fact; a rule on that fact needs a `docs/plan.md` item |
| T1070.004 File Deletion (319) | Strong 2 | **This tool, later** | `$J` delete records are facts this tool already exposes; rules come with the item that needs them |
| T1561.001/.002 Disk wipe | 0 rules | Drop | `raw_access_thread_susp_disk_access_using_uncommon_tools` (T1006) and 16 strong T1490 rules cover the same intrusions |
| Security 4670 | 53 analytics, 0 rules | Drop | 52 of 53 analytics cite the same boilerplate `EventCode=4663, 4670, 4656`; generic permission changes fail G3 |
| 4726/4733/4757/4740/4798 | 0 rules | Drop | Informational at best; Hayabusa covers 4720/4728/4732 |
| T1113 (172), T1056.001 (155) | Strong 0 | Other layer (EDR) | API-level behavior absent from event logs (G3) |
| T1027.002/.013/.016 (105/256/27) | 0–1 rules | Other layer (YARA) | File content, not events (G5) |
| T1041, T1071.x, T1573.x, T1001, T1030 | 0–2 rules | Other layer (NSM) | Network content (G5) |
| T1074.001 (144), T1560.001 (89) | Strong 0/3 | Hold | No signal that passes G3 without a staging-path baseline; revisit with measurement |

## New rules: 2 (both correlations)

| ID | Rule | Done when |
|---|---|---|
| C1 | Sigma `event_count` over existing discovery rules, `group-by` host + user, short timespan. Base rules stay `informational`/`low`; C1 is `medium` until FP is measured | Threshold and timespan chosen from a clean baseline and one regression sample; 0 baseline hits |
| C2 | Hayabusa's `Sec_4625_*_Correlation` (×2) and `Sec_4648_*_Correlation` as Sigma correlations | Same matches as the Hayabusa originals on one spray/guessing sample EVTX |

## Fixes without new rules

| ID | Change | Count |
|---|---|---|
| T1 | Hayabusa: revoked technique tags (`T1070.001`→`T1685.005` ×3, `T1562.002`→`T1685.001`, `T1562.010`→`T1689`) | 5 |
| T2 | Hayabusa: `attack.defense-evasion` → `stealth` / `defense-impairment` (v20 split) | 17 |
| T3 | Hayabusa: `Sysmon_6_*` tactic/technique mismatch (T1553.002 is now defense-impairment) | 5 |
| T4 | Hayabusa: builtin rules without a technique tag | 156 |
| T5 | Sigma: add `attack.t1680` to the two rules above | 2 |
| T6 | Lint in CI: tags vs ATT&CK STIX (revoked, deprecated, tactic mismatch), ATT&CK version pinned | 1 check |

## Removal and demotion candidates

| ID | Candidate | Count |
|---|---|---|
| R1 | Sigma `high`/`critical` with FP "Unknown" and no `regression_data` → `medium` until either exists | 853 |
| R2 | Hayabusa `*_Cnt_Deprecated.yml` (replaced by the correlation versions) → delete | 3 |
| R3 | Yara-Rules set → freeze; do not extend. Replacing it is a dependency decision (H2) | 1 set |

## Caveats

- A tag is not a detection. Every Done-when above is measured with `regression_data` or a baseline, not by tag counts.
- Hayabusa, SigmaHQ and Yara-Rules are upstream projects; T1–T5, C2 and R1–R2 are contributions there, not changes in this repository.
