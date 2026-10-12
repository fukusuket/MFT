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
