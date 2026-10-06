# ADR 0011: Which time is "created" for an MFT entry

Status: accepted (2026-10-06, approved by the maintainer)

## Context
An MFT entry has two creation times. `$STANDARD_INFORMATION` (`$SI`) is the one Windows APIs show and user-mode tools can set (timestomping). `$FILE_NAME` (`$FN`) is set by the kernel on create or rename and is rarely changed by tools. Sigma `file_event` rules use `CreationUtcTime`, which Sysmon fills with the API-visible (`$SI`-equivalent) time. product.md §7 Q1 leans to "in window if either is". P1-1 (CSV) and P1-4 (Sigma events) need one answer.

## Decision
| Use | Time |
|---|---|
| Sigma `CreationUtcTime` | `$SI` created. Same meaning as Sysmon, so SigmaHQ rules keep their semantics |
| Extra event field `FnCreationUtcTime` | `$FN` created, from the name chosen for the path (Win32 / Win32+DOS, then POSIX, then DOS) |
| Event timestamp for the engine (`process_event_at`, ordering) | `$SI` created |
| Time window | An entry is in the window if **either** `$SI` or `$FN` created is in it |
| CSV and report | Both times at full 100 ns precision (`Filetime`, UTC) |

- Sigma field format: `YYYY-MM-DD HH:MM:SS.fff` in UTC, as Sysmon writes it. This truncates to milliseconds, which is deterministic.
- No comparison between the two times in Rust. A fact such as "`$SI` created earlier than `$FN` created" is Phase 2 work (facts), and rules decide what it means.

Not chosen:
- `$FN` as `CreationUtcTime`: it changes the meaning of SigmaHQ rules.
- `$SI` only for the window: a timestomped file would leave the window and disappear from the report.

## Consequences
- P1-1 CSV: columns `si_created` and `fn_created`.
- P1-4: `FnCreationUtcTime` is a project-specific field. Document it with the other non-Sysmon fields when the event schema is written.
- product.md §7 Q1 is answered; remove the row when this ADR is accepted.
