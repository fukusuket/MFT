# ADR 0020: Viewer in plain JS with a virtual-scroll table

Status: accepted (2026-10-10, approved by the maintainer)

## Context
P2-9 needs a viewer that stays responsive on many rows, in one offline HTML file (product.md P6), with byte-identical output and minimal supply-chain surface (ADR 0007). Clients produce about 0.5–0.75 M MFT + USN rows; plain JS handles 1 M rows interactively, and the limits are payload size and memory, not compute. Evidence: [research/viewer.md](../research/viewer.md).

## Decision
| Area | Decision |
|---|---|
| Build | Plain JS and CSS inside `crates/report/src/template.html`. No framework, no npm, no build step, no WASM |
| Rendering | DOM virtual scroll: a fixed pool of row elements, text set with `textContent` only; the spacer height is capped below Firefox's element height limit and the scroll position is mapped onto the row range |
| Data | Base64 JSON embedded as today, decoded with `Uint8Array.fromBase64` and a plain-loop fallback |
| Scope of embedded rows | Rows in the time window (P2-8); out-of-window rows stay in CSV/JSONL |
| Order | Rows arrive from Rust in the report order (level → time); the viewer sorts by other columns on request with a total, ordinal comparison |
| Escalation | Each step only when a measurement on a real host requires it, each by its own ADR: (1) gzip + `DecompressionStream` (one compression crate), (2) binary columns with a string dictionary, chunked loading and a worker, (3) Perspective or WASM |

Rejected: Svelte or another framework; grid libraries (AG Grid, Glide Data Grid, Tabulator, SlickGrid); DuckDB-Wasm, sql.js, Arrow JS; own Rust compiled to WASM; canvas rendering; a separate data file. Reasons in the research note.

## Consequences
- No new dependency; nothing changes in `deny.toml`, `supply-chain/` or `NOTICE`.
- `crates/report/src/template.html` grows into the viewer in P2-9; the existing no-`innerHTML`, no-network test still applies.
- `docs/architecture.md`: the `report` row drops "Svelte viewer"; the summary line drops "no Svelte yet".
- `docs/plan.md`: the "ADR: viewer build" item is ticked with a link to this ADR; P2-9's Done when gains a performance check (e.g. 1 M synthetic rows: filter and search ≤ 200 ms, no dropped content when scrolling to the last row in Chrome and Firefox).
- Real-browser scroll smoothness and `file://` behaviour of workers and WASM remain to be measured; they only matter for the escalation steps.
