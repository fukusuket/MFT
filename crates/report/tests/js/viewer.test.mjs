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
