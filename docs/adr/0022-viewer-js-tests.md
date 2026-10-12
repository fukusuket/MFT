# ADR 0022: Test the viewer's JS under Node, without npm

Status: proposed

## Context
P2-9b turns `crates/report/src/template.html` into the viewer (ADR 0020): filters, an inclusive `to` in the time range, a URL-hash round trip and the row mapping of a capped virtual scroll. These are easy to get wrong, and nothing runs them today: the Rust tests only check that the template uses no `innerHTML`, loads nothing, and embeds the right JSON. The JS has to run somewhere to be tested, and every option adds a tool. Adding a CI tool is an H2 decision.

## Decision
| Area | Decision |
|---|---|
| Runtime | Node (LTS, ≥ 22), its built-in `node:test`, `node:assert` and `node:vm` only. No npm, no `package.json`, no lock file, no `node_modules` |
| Code layout | The viewer's script moves to `crates/report/src/viewer.js`. Rust inlines it into the template at compile time (`include_str!`, like the template), so the report is still one self-contained file and stays byte-identical |
| Testable shape | `viewer.js` keeps its logic in plain functions (filter, time range, hash parse/format, scroll row mapping). It renders only when a `document` is present, so tests can load it with `vm` and call the functions |
| DOM | A small hand-written stub in the test folder (`createElement`, `appendChild`, `textContent`, `getElementById`, `location.hash`, events); no jsdom |
| Tests | `crates/report/tests/js/*.test.mjs`, run with `node --test crates/report/tests/js/` |
| CI | One new job on `ubuntu-latest`: `actions/setup-node` pinned to a full commit SHA (ADR 0007 P8), `node-version: 22`, then the command above. `permissions: contents: read`, `persist-credentials: false` |
| Local | The command joins the verify list in `AGENTS.md`; contributors need Node 22 to run it |
| Rust tests | Unchanged: no-`innerHTML`, no network, attacker names and the JSON contract stay in Rust. Node is never needed for `cargo` |

Not chosen:
- Running Node from a Rust test under `cargo nextest`: every `cargo test` on every OS would need Node.
- jsdom or another npm test stack: brings a dependency tree that `cargo deny`/`cargo vet` do not cover.
- Moving the logic to Rust and keeping JS thin: the filters and scroll still run in the browser, so their JS still needs tests.
- Manual checks only: no regression protection.
- A headless browser in CI: heavy, and only P2-9c (scroll in real browsers) needs one; that stays a human check.

## Consequences
- `deny.toml`, `supply-chain/` and `NOTICE` are unchanged (no crate, no npm package). The new Action is pinned and updated by Dependabot like the others; `zizmor` lints the job.
- `crates/report/src/template.html` loses its inline script; the existing template tests read the inlined result instead.
- JS line coverage is not measured (`cargo llvm-cov` covers Rust only).
- `AGENTS.md` and `CLAUDE.md` mention the JS test command; `docs/plan.md` ticks the ADR item and links this ADR.
- The CI job and the first tests land with P2-9b, which brings the code they test.
