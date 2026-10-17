// Viewer timings at scale (P2-9c, ADR 0020); not a test. Rows of a real report are copied up
// to the asked counts, rendered with the test DOM stand-in, then each filter is timed.
//
//   node --max-old-space-size=8192 crates/report/tests/js/bench.mjs <report.html> [outside] [findings]
import { readFileSync } from "node:fs";
import { performance } from "node:perf_hooks";
import { load } from "./load.mjs";
import { page } from "./dom.mjs";

const [file, outsideCount = "1000000", findingsCount = "341140"] = process.argv.slice(2);
const html = readFileSync(file, "utf8");
const base = load().decode(html.match(/id="data">([^<]*)</)[1]);

// `count` rows cycling through `rows`, each with its own MFT entry.
const grow = (rows, count) =>
  Array.from({ length: Number(count) }, (_, i) => ({ ...rows[i % rows.length], entry: 1000000 + i }));
const data = {
  ...base,
  outside: grow(base.outside, outsideCount),
  findings: grow(base.findings, findingsCount),
};
const payload = Buffer.from(JSON.stringify(data)).toString("base64");

const ms = (start) => Math.round(performance.now() - start);
const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];

let t = performance.now();
const { document, window } = page({ app: "", data: payload, version: "", commit: "", license: "", source: "" }, "#tab=findings");
load().render(document, window);
const results = [["load: decode, parse, search texts, first Findings draw", ms(t)]];
const heap = Math.round(process.memoryUsage().heapUsed / 2 ** 20);

const app = document.getElementById("app");
const screen = (name) => app.findAll("screen").find((s) => s.getAttribute("data-screen") === name);
const choose = (name, key, value) => {
  const select = screen(name).find("filter-" + key);
  select.value = value;
  select.dispatch("change");
};
const type = (name, key, value) => {
  const input = screen(name).find("filter-" + key);
  input.value = value;
  input.dispatch("input");
};
const timed = (label, action, reset, runs = 5) => {
  const times = [];
  for (let i = 0; i < runs; i++) {
    const start = performance.now();
    action();
    times.push(performance.now() - start);
    reset();
  }
  results.push([label, Math.round(median(times))]);
};
const clear = (name) => screen(name).find("clear").dispatch("click");

const level = base.findings[0].level;
const folder = base.outside[0].folder;
timed("findings: level", () => choose("findings", "level", level), () => clear("findings"));
timed("findings: folder", () => choose("findings", "folder", base.findings[0].folder), () => clear("findings"));
timed("findings: rule", () => choose("findings", "rule", base.findings[0].id), () => clear("findings"));
timed("findings: time from", () => type("findings", "from", base.findings[0].created?.slice(0, 10) ?? "2000-01-01"), () => clear("findings"));
timed("findings: search, first", () => type("findings", "q", "exe"), () => clear("findings"), 1);
timed("findings: search", () => type("findings", "q", "svchost"), () => clear("findings"));

t = performance.now();
app.findAll("tab")[2].dispatch("click");
results.push(["outside: open tab (filter + draw)", ms(t)]);
timed("outside: folder", () => choose("outside", "folder", folder), () => clear("outside"));
timed("outside: search, first", () => type("outside", "q", "exe"), () => clear("outside"), 1);
timed("outside: search", () => type("outside", "q", "svchost"), () => clear("outside"));

console.log(`rows: outside ${data.outside.length}, findings ${data.findings.length}; payload ${Math.round(payload.length / 2 ** 20)} MiB Base64; heap after load ${heap} MiB`);
for (const [label, value] of results) console.log(`${String(value).padStart(6)} ms  ${label}`);
