// Loads viewer.js into a fresh context, as the report's inline script would run.
import { readFileSync } from "node:fs";
import vm from "node:vm";

const source = readFileSync(new URL("../../src/viewer.js", import.meta.url), "utf8");

/** The `viewer` object; `globals` become the context's globals (e.g. a `document` stub). */
export function load(globals = {}) {
  const context = vm.createContext({ TextDecoder, ...globals });
  return vm.runInContext(`${source}\nviewer;`, context);
}

/** Runs `code` in a fresh context first (e.g. to change a builtin), then loads the viewer. */
export function loadAfter(code, globals = {}) {
  const context = vm.createContext({ TextDecoder, ...globals });
  vm.runInContext(code, context);
  return vm.runInContext(`${source}\nviewer;`, context);
}
