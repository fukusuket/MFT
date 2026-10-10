# Viewer: handling many rows

Surveyed 2026-10-10. Question: which viewer technology keeps a large timeline responsive in one offline HTML file (product.md P6), with the fewest moving parts? Decision: [ADR 0020](../adr/0020-viewer-build.md).

## Expected size

Measured on the local corpora (12 hosts). `$MFT` rows = file size / 1 KiB (all entries, in use or not); USN rows counted by walking `RecordLength`.

| Hosts | `$MFT` | MFT rows | `$J` data | USN rows | Total |
|---|---|---|---|---|---|
| Yamato CTF, 9 × Win7 clients | 91–92 MB | ≈ 90 k | 34–38 MB | 345–382 k | ≈ 0.45 M |
| Simulated-Case-1 `desktop6`, Win11 24H2 | 133 MB | 131 k | 36 MB | 347 k | ≈ 0.48 M |
| Simulated-Case-1 `dc1` (domain controller) | 128 MB | 126 k | 67 MB | 607 k | ≈ 0.73 M |
| Simulated-Case-1 `files5` (file server) | 127 MB | 124 k | 39 MB | 346 k | ≈ 0.47 M |

- A USN record averages ~100 bytes, so ~10 k rows per MB. The client default journal size (32 MB) caps USN at roughly 0.35–0.6 M rows, independent of disk size.
- Estimates beyond the corpus: a heavily used workstation 0.3–1 M MFT rows (the Phase 1 gate uses a 1 GB `$MFT`); large file servers reach millions to 10 M+ (out of MVP scope, product.md §7 #5). The small lab file server above is not representative of production ones.
- One row per record with timestamps as columns keeps these numbers. Expanding each timestamp into its own row (MACB style) multiplies rows by 4–8.

## Measurements

Node 22 (V8, as in Chrome) on the maintainer's Mac. 1 M rows = Simulated-Case-1's 126,913 `tool analyze` rows copied 8 times; current columns only. Browser paint and scroll smoothness are not measured (headless Chrome could not start inside the agent sandbox).

| Operation, 1 M rows | Time |
|---|---|
| Base64 decode + `JSON.parse` | 0.9 s (heap ≈ 400 MB) |
| gzip via `DecompressionStream` + `JSON.parse` | 1.2 s |
| Lowercase all paths once | 76 ms |
| Substring search on path | 43 ms |
| Regex search on path | 12 ms |
| Filter by level | 2 ms |
| Sort by path / by time | 385 / 256 ms |

| Payload, 1 M rows | Size |
|---|---|
| Row-oriented JSON | 380 MB (≈ 500 MB as Base64) |
| Column-oriented JSON | 278 MB |
| gzip of either | 33–36 MB |

- Filtering and search are interactive in plain JS at 1 M rows. Sorting can be done in Rust ahead of time.
- Columns alone barely shrink the payload; strings dominate. Size drops only with a string dictionary or compression.
- The current template's `Uint8Array.from(atob(..), fn)` ran out of memory at ≈ 280 MB. `Uint8Array.fromBase64` (Baseline 2025: Chrome 140, Firefox 133, Safari 18.2) or a plain loop avoids it.
- A sample viewer (plain JS, capped virtual scroll, presets, search, sort, URL hash) logic-tested against a DOM stub: 127 k real rows load in 0.17 s; at 1 M rows sort is 0.3 s, a search including the first lowercase pass 0.18 s, a filter 3 ms.

## Rendering

| Option | Many rows | Simplicity | Caveat | Verdict |
|---|---|---|---|---|
| Own DOM virtual scroll | Only ~40 visible rows touched; row count does not matter | Few hundred lines, no dependency | Element height cap (below) | Chosen |
| Own canvas grid | Best with very many columns | Must rebuild selection, copy, find-in-page | Large effort | Rejected |
| Glide Data Grid (MIT) | Canvas, millions of rows claimed | Needs React | Dependency tree | Rejected |
| AG Grid Community (MIT) | Good | Large | A competitor's test reports ≈ 1.3 GB heap at 100 k rows and a crash past 300 k | Rejected |
| Tabulator / SlickGrid (MIT) | 100 k – several 100 k rows | Medium | SlickGrid development is slow | Rejected |

Element height cap: Firefox ignores heights above 17,895,697 px; Chrome and Safari clamp near 33,554,431 px. At 20 px per row Firefox fits ~0.89 M rows. Cap the spacer (e.g. 8 M px) and map the scroll position onto the row range.

## Data and query engine

| Option | 1 M rows | 10 M rows | Simplicity | Verdict |
|---|---|---|---|---|
| Row JSON + JS arrays | Yes (measured) | No: one JSON string exceeds V8's ≈ 512 MB string limit | Simplest | Now |
| Columns + string dictionary + typed arrays | Yes, less memory | Yes, if chunked | One more output format in Rust | When memory demands |
| Apache Arrow JS (Apache-2.0) | Yes | Yes | One dependency | Rejected |
| DuckDB-Wasm / sql.js (MIT) | Yes, SQL | Yes | Several MB of WASM plus a worker; `file://` behaviour unverified | Rejected |
| Perspective (FINOS, Apache-2.0) | Yes | Yes (C++ WASM, memory64, OPFS paging) | Large | Candidate only at 10 M rows |
| Own Rust compiled to WASM | Yes | Yes | Extra build, CSP `wasm-unsafe-eval`, committed artifact for byte-identical output; string marshalling to the DOM cancels the gain | Rejected |

## Delivery and parallelism

| Option | Note | Verdict |
|---|---|---|
| Base64 JSON in the HTML (current) | ≈ 500 MB at 1 M rows; fine for in-window rows | Now |
| gzip + `DecompressionStream` | ≈ 45 MB at 1 M rows; needs a compression crate in Rust (H2) | When all rows are embedded |
| Separate data file, drag and drop | Breaks the one-file report (P6) | Last resort |
| Web Worker from a Blob URL | Not needed at measured sizes; needs CSP `worker-src blob:`; `file://` unverified | Later, if measured |
| `SharedArrayBuffer` | Requires `crossOriginIsolated`, which needs response headers that `file://` cannot send (inferred, not tested) | Not possible |

## Staging

| Scale | Build | New dependency |
|---|---|---|
| Up to a few 100 k rows (time window) | Plain JS, DOM virtual scroll, Base64 JSON | None |
| ≈ 1 M rows (all rows) | Add gzip, `fromBase64`, pre-sorted columns from Rust | One compression crate |
| 10 M rows | Binary columns with a string dictionary, chunked loading, a worker; reconsider Perspective or WASM | Revisit |

## Open

- Paint and scroll smoothness in real browsers (Chrome, Firefox, Safari), including the capped spacer.
- Blob workers and WASM instantiation under `file://`.

## Sources

- Element height limits: [meyerweb, 2025](https://meyerweb.com/eric/thoughts/2025/08/07/infinite-pixels/), [W3C CSS archive, 2018](https://lists.w3.org/Archives/Public/public-css-archive/2018Dec/0503.html)
- `Uint8Array.fromBase64`: [MDN](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/Uint8Array/fromBase64), [caniuse](https://caniuse.com/mdn-javascript_builtins_uint8array_frombase64)
- Perspective: [GitHub](https://github.com/finos/perspective), [JS guide](https://perspective.finos.org/guide/explanation/javascript.html)
- Glide Data Grid: [README](https://cdn.jsdelivr.net/gh/glideapps/glide-data-grid@main/README.md), [docs](https://docs.grid.glideapps.com)
- DuckDB-Wasm: [overview](https://www.duckdb.org/docs/current/clients/wasm/overview), [announcement](https://duckdb.org/2021/10/29/duckdb-wasm)
- Grid comparisons: [RevoGrid benchmark (vendor)](https://dev.to/revolist/battle-of-the-rows-the-limits-of-data-performance-4mcn), [RevoGrid alternatives (vendor)](https://rv-grid.com/compare/ag-grid-alternatives), [1771 Technologies (vendor)](https://www.1771technologies.com/blog/performance-benchmarks)
- Apache Arrow JS: [arrow.apache.org/js](https://arrow.apache.org/js/)
