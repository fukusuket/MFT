import { test } from "node:test";
import assert from "node:assert/strict";
import { load, loadAfter } from "./load.mjs";

const base64 = (value) => Buffer.from(JSON.stringify(value), "utf8").toString("base64");

test("decode reads Base64 JSON with or without Uint8Array.fromBase64", () => {
  const value = { path: "\\Users\\bob\\日本語.txt", n: [1, 2, 3] };
  for (const payload of [base64(value), ` ${base64(value)}\n`, base64({ a: "x" }), base64({ ab: "y" })]) {
    const expected = JSON.parse(Buffer.from(payload.trim(), "base64").toString("utf8"));
    const fallback = load().decode(payload);
    // A stand-in for the browsers' Uint8Array.fromBase64, which Node 22 lacks.
    const native = loadAfter("Uint8Array.fromBase64 = (text) => Uint8Array.from(bytesOf(text));", {
      bytesOf: (text) => [...Buffer.from(text, "base64")],
    }).decode(payload);
    assert.deepEqual(JSON.parse(JSON.stringify(fallback)), expected);
    assert.deepEqual(JSON.parse(JSON.stringify(native)), expected);
  }
});

test("the URL hash round-trips the tab and filters, and ignores what it does not know", () => {
  const { parseHash, formatHash } = load();
  const state = {
    tab: "findings",
    level: "high",
    rule: "rule-1",
    folder: "downloads",
    from: "2026-10-01",
    to: "2026-10-02T12:00:00",
    q: "a#b&c=d %e\\f",
  };

  const hash = formatHash(state);

  assert.ok(hash.startsWith("#"));
  assert.deepEqual({ ...parseHash(hash) }, state);
  assert.deepEqual(
    { ...parseHash("#tab=outside&bogus=1&q=%E0%A4%A&folder=temp") },
    { tab: "outside", level: "", rule: "", folder: "temp", from: "", to: "", q: "" },
  );
  assert.deepEqual({ ...parseHash("") }, { tab: "summary", level: "", rule: "", folder: "", from: "", to: "", q: "" });
  assert.equal(formatHash({ ...parseHash(""), tab: "findings" }), "#tab=findings");
});

const none = { tab: "findings", level: "", rule: "", folder: "", from: "", to: "", q: "" };
const finding = (over) => ({
  level: "high",
  id: "rule-1",
  folder: "downloads",
  path: "\\Users\\Bob\\Downloads\\Tool.EXE",
  created: "2026-10-02T12:00:00.5000000Z",
  fn_created: "2026-09-01T00:00:00.0000000Z",
  event_time: null,
  ...over,
});

test("findings match the level, rule, folder and a case-insensitive path search", () => {
  const { matchFinding } = load();
  const f = finding({});

  assert.equal(matchFinding(f, none), true);
  for (const [key, hit, miss] of [
    ["level", "high", "low"],
    ["rule", "rule-1", "rule-2"],
    ["folder", "downloads", "temp"],
    ["q", "bob\\downloads\\tool.exe", "public"],
  ]) {
    assert.equal(matchFinding(f, { ...none, [key]: hit }), true, `${key}=${hit}`);
    assert.equal(matchFinding(f, { ...none, [key]: miss }), false, `${key}=${miss}`);
  }
  assert.equal(matchFinding(f, { ...none, level: "high", folder: "temp" }), false);
});

test("the time range holds from, the whole to-day or to-second, on any of the row's times", () => {
  const { matchFinding } = load();
  const f = finding({}); // $SI 2026-10-02T12:00:00.5Z, $FN 2026-09-01
  const usn = finding({ created: null, fn_created: null, event_time: "2026-10-03T00:00:00.0000000Z" });
  const at = (from, to, row = f) => matchFinding(row, { ...none, from, to });

  assert.equal(at("2026-10-02T12:00:00", ""), true); // from holds its second
  assert.equal(at("2026-10-02T12:00:01", ""), false);
  assert.equal(at("", "2026-10-02T12:00:00"), true); // to holds its whole second
  assert.equal(at("", "2026-10-02"), true); // and its whole day
  assert.equal(at("", "2026-08-31"), false);
  assert.equal(at("2026-09-01", "2026-09-01"), true); // only $FN is in
  assert.equal(at("2026-10-03", "2026-10-03", usn), true); // only the USN time is in
  assert.equal(at("2026-10-04", "", usn), false);
  assert.equal(at("2026/10/04", "2026-10-1", usn), true); // malformed bounds are ignored
});

test("only YYYY-MM-DD and YYYY-MM-DDTHH:MM:SS are valid times", () => {
  const { validTime } = load();
  for (const text of ["", "2026-10-02", "2026-10-02T23:59:59"]) assert.equal(validTime(text), true, text);
  for (const text of ["2026/10/02", "2026-10-2", "2026-10-02T1:00:00", "2026-10-02 12:00:00", "x"]) {
    assert.equal(validTime(text), false, text);
  }
});

test("outside-baseline files match the folder, the time range and the path search only", () => {
  const { matchOutside } = load();
  const o = {
    created: "2026-10-02T12:00:00.5000000Z",
    fn_created: "2026-09-01T00:00:00.0000000Z",
    path: "\\Users\\Bob\\Downloads\\Tool.EXE",
    entry: 40,
    folder: "downloads",
  };

  assert.equal(matchOutside(o, { ...none, level: "critical", rule: "other" }), true);
  assert.equal(matchOutside(o, { ...none, folder: "downloads", q: "TOOL" }), true);
  assert.equal(matchOutside(o, { ...none, folder: "temp" }), false);
  assert.equal(matchOutside(o, { ...none, q: "public" }), false);
  assert.equal(matchOutside(o, { ...none, from: "2026-09-01", to: "2026-09-01" }), true); // by $FN
  assert.equal(matchOutside(o, { ...none, from: "2026-10-03" }), false);
});

test("rowRange maps the scroll position to rows, below and above the spacer cap", () => {
  const { rowRange } = load();
  const range = (...args) => ({ ...rowRange(...args) });

  // 100 rows of 20 px in a 200 px viewport: no cap, rows sit at their own offset.
  assert.deepEqual(range(0, 200, 20, 100, 8_000_000), { first: 0, last: 11, top: 0 });
  assert.deepEqual(range(205, 200, 20, 100, 8_000_000), { first: 10, last: 21, top: 200 });
  assert.deepEqual(range(1800, 200, 20, 100, 8_000_000), { first: 90, last: 100, top: 1800 });

  // 1 M rows need 20 M px; the spacer stops at 8 M px and the rows follow the viewport.
  const top = range(0, 400, 20, 1_000_000, 8_000_000);
  assert.deepEqual(top, { first: 0, last: 21, top: 0 });
  const end = range(8_000_000 - 400, 400, 20, 1_000_000, 8_000_000);
  assert.deepEqual(end, { first: 999_980, last: 1_000_000, top: 8_000_000 - 400 });
  const middle = range(4_000_000, 400, 20, 1_000_000, 8_000_000);
  assert.ok(middle.first > 400_000 && middle.first < 600_000, JSON.stringify(middle));
  assert.equal(middle.top, 4_000_000);

  // Fewer rows than fit.
  assert.deepEqual(range(0, 400, 20, 3, 8_000_000), { first: 0, last: 3, top: 0 });
  assert.deepEqual(range(0, 400, 20, 0, 8_000_000), { first: 0, last: 0, top: 0 });
});

import { page } from "./dom.mjs";

// Report data in the shape the Rust side writes.
const report = {
  summary: {
    input: "MFT",
    host: "not available ($MFT only)",
    record_size: 1024,
    entries: 5,
    files: 3,
    resolved: 4,
    unknown: 1,
    diagnostics: { bad_signature: 0, fixup_mismatch: 0, malformed: 0, truncated: 0, orphan_extension: 0 },
    baseline: "w11.fst",
    outside: 2,
    rules: 3,
    skipped_rules: 1,
    window: null,
    warning: null,
    usn_first: "2026-10-01T00:00:00.0000000Z",
    usn_last: "2026-10-03T00:00:00.0000000Z",
    findings: { critical: 0, high: 1, medium: 0, low: 1, informational: 0, none: 0 },
  },
  findings: [
    finding({ title: "Tool in Downloads", author: null, explain: "{}", usn: null, path_state: "resolved", entry: 40, deleted: false, seen: "mft", streams: ["Zone.Identifier"], hints: [] }),
    finding({ level: "low", id: "rule-2", folder: "temp", path: "\\Windows\\Temp\\x.dll", title: "DLL in Temp", author: "a", explain: "{}", usn: 64, path_state: "inferred", entry: 41, deleted: true, seen: "usn", streams: [], hints: [] }),
  ],
  outside: [
    { created: "2026-10-02T12:00:00.0000000Z", fn_created: null, path: "\\Users\\Bob\\Downloads\\Tool.EXE", entry: 40, folder: "downloads" },
    { created: "2026-10-01T00:00:00.0000000Z", fn_created: null, path: "\\Windows\\Temp\\x.dll", entry: 41, folder: "temp" },
  ],
  footer: { version: "1.0.0", commit: "abc", source_url: "https://example.com", license: "AGPL-3.0-only" },
};

function open(hash) {
  const { document, window } = page(
    { app: "", data: base64(report), version: "", commit: "", license: "", source: "" },
    hash,
  );
  load().render(document, window);
  const app = document.getElementById("app");
  const shown = () => app.findAll("screen").filter((s) => s.style.display !== "none").map((s) => s.getAttribute("data-screen"));
  return { document, window, app, shown };
}

test("the URL hash chooses the tab, and the tabs change the hash", () => {
  const { window, app, shown } = open("#tab=findings");

  assert.deepEqual(shown(), ["findings"]);
  window.go("#tab=outside");
  assert.deepEqual(shown(), ["outside"]);
  window.go("");
  assert.deepEqual(shown(), ["summary"]);
  const tabs = app.findAll("tab");
  assert.deepEqual(tabs.map((t) => t.textContent), ["Summary", "Findings (2)", "Outside-baseline (2)"]);
  tabs[1].dispatch("click");
  assert.equal(window.location.hash, "#tab=findings");
  assert.deepEqual(shown(), ["findings"]);
});

test("Findings: filters change the hash, the N of M count and the rows; a row opens its detail", () => {
  const { window, app } = open("#tab=findings&level=high");
  const screen = app.findAll("screen").find((s) => s.getAttribute("data-screen") === "findings");
  const count = () => screen.find("count").textContent;
  const rows = () => screen.findAll("row");
  const control = (name) => screen.find("filter-" + name);

  assert.equal(count(), "1 of 2");
  assert.equal(rows().length, 1);
  assert.equal(control("level").value, "high");
  const row = rows()[0].textContent;
  for (const text of ["high", "2026-10-02T12:00:00.5000000Z", "\\Users\\Bob\\Downloads\\Tool.EXE", "downloads", "Tool in Downloads", "ADS: Zone.Identifier", "seen: mft"]) {
    assert.ok(row.includes(text), `${text} in ${row}`);
  }

  control("level").value = "";
  control("level").dispatch("change");
  assert.equal(count(), "2 of 2");
  assert.equal(window.location.hash, "#tab=findings");

  control("q").value = "TEMP";
  control("q").dispatch("input");
  assert.equal(count(), "1 of 2");
  assert.ok(rows()[0].textContent.includes("deleted"));
  assert.ok(rows()[0].textContent.includes("\\Windows\\Temp\\x.dll (inferred)"));

  control("from").value = "2026/10";
  control("from").dispatch("input");
  assert.equal(count(), "1 of 2");
  assert.ok(control("from").className.includes("invalid"));

  rows()[0].dispatch("click");
  assert.ok(screen.find("detail").textContent.includes("DLL in Temp"));
});

test("Summary: the critical and high counts open Findings filtered to that level", () => {
  const { window, app, shown } = open("");
  const links = app.findAll("level-link");

  assert.deepEqual(links.map((l) => l.textContent), ["critical 0", "high 1"]);
  links[1].dispatch("click");
  assert.equal(window.location.hash, "#tab=findings&level=high");
  assert.deepEqual(shown(), ["findings"]);
});

test("Outside-baseline: per-folder counts filter the list, with time and path filters", () => {
  const { window, app } = open("#tab=outside");
  const screen = app.findAll("screen").find((s) => s.getAttribute("data-screen") === "outside");
  const count = () => screen.find("count").textContent;
  const rows = () => screen.findAll("row");
  const links = screen.findAll("folder-link");

  assert.equal(count(), "2 of 2");
  assert.deepEqual(links.map((l) => l.textContent), ["downloads 1", "temp 1"]);
  assert.equal(screen.find("filter-level"), null); // levels and rules are for findings
  links[1].dispatch("click");
  assert.equal(window.location.hash, "#tab=outside&folder=temp");
  assert.equal(count(), "1 of 2");
  for (const text of ["2026-10-01T00:00:00.0000000Z", "\\Windows\\Temp\\x.dll", "41"]) {
    assert.ok(rows()[0].textContent.includes(text), text);
  }

  screen.find("filter-folder").value = "";
  screen.find("filter-folder").dispatch("change");
  screen.find("filter-to").value = "2026-10-01";
  screen.find("filter-to").dispatch("input");
  assert.equal(count(), "1 of 2");
  assert.equal(window.location.hash, "#tab=outside&to=2026-10-01");
});

test("rowRange covers the whole viewport when it is not a multiple of the row height", () => {
  const { rowRange } = load();
  for (const scrollTop of [0, 1, 43, 44, 300]) {
    const r = rowRange(scrollTop, 500, 44, 100, 8_000_000);
    assert.ok(r.top + (r.last - r.first) * 44 >= scrollTop + 500, `${scrollTop}: ${JSON.stringify(r)}`);
  }
});

test("a filter value from the hash that the report lacks stays visible in its select", () => {
  const { app } = open("#tab=findings&rule=old-id");
  const screen = app.findAll("screen").find((s) => s.getAttribute("data-screen") === "findings");
  const select = screen.find("filter-rule");

  assert.equal(screen.find("count").textContent, "0 of 2");
  const options = select.children.map((o) => [o.value, o.textContent]);
  assert.deepEqual(options.at(-1), ["old-id", "old-id (not in this report)"]);
  assert.equal(select.value, "old-id");
});
