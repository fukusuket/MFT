"use strict";
// The viewer: plain functions, and `render`, which draws the page. It renders only when a
// document is present, so tests can load this file under Node and call the functions.
const viewer = (function () {
  const BASE64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

  // Base64 without `atob`, whose string copy runs out of memory on large reports.
  function fromBase64(text) {
    const out = new Uint8Array(Math.floor((text.replace(/=+$/, "").length * 3) / 4));
    let bits = 0;
    let count = 0;
    let at = 0;
    for (let i = 0; i < text.length && text[i] !== "="; i++) {
      bits = (bits << 6) | BASE64.indexOf(text[i]);
      count += 6;
      if (count >= 8) {
        count -= 8;
        out[at++] = (bits >> count) & 0xff;
      }
    }
    return out;
  }

  // The embedded report data: Base64 of UTF-8 JSON.
  function decode(payload) {
    const text = payload.trim();
    const bytes = typeof Uint8Array.fromBase64 === "function" ? Uint8Array.fromBase64(text) : fromBase64(text);
    return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  }

  // The tab and filters, kept in the URL hash so a reload or a shared link shows the same view.
  const KEYS = ["tab", "level", "rule", "folder", "from", "to", "q"];

  function parseHash(hash) {
    const state = { tab: "summary", level: "", rule: "", folder: "", from: "", to: "", q: "" };
    for (const part of hash.replace(/^#/, "").split("&")) {
      const at = part.indexOf("=");
      const key = part.slice(0, at);
      if (at < 0 || !KEYS.includes(key)) continue;
      try {
        state[key] = decodeURIComponent(part.slice(at + 1));
      } catch (e) {
        // a malformed value is ignored like an unknown key
      }
    }
    return state;
  }

  function formatHash(state) {
    const parts = KEYS.filter((key) => state[key] !== "" && !(key === "tab" && state[key] === "summary"))
      .map((key) => key + "=" + encodeURIComponent(state[key]));
    return "#" + parts.join("&");
  }

  // `YYYY-MM-DD` or `YYYY-MM-DDTHH:MM:SS` (UTC); empty means unset.
  function validTime(text) {
    return /^(\d{4}-\d{2}-\d{2}(T\d{2}:\d{2}:\d{2})?)?$/.test(text);
  }

  // Whether any of `times` (ISO 8601, UTC) is in the range. A time is cut to the bound's length,
  // so `to` holds its whole day or second. Malformed bounds are ignored.
  function inRange(times, state) {
    const from = validTime(state.from) ? state.from : "";
    const to = validTime(state.to) ? state.to : "";
    if (from === "" && to === "") return true;
    return times.some(
      (t) => t !== null && (from === "" || t.slice(0, from.length) >= from) && (to === "" || t.slice(0, to.length) <= to),
    );
  }

  // The filters both lists share: folder and a case-insensitive path search.
  function matchPlace(row, state) {
    return (
      (state.folder === "" || row.folder === state.folder) &&
      (state.q === "" || row.path.toLowerCase().includes(state.q.toLowerCase()))
    );
  }

  // Whether a finding passes every filter that is set.
  function matchFinding(f, state) {
    return (
      (state.level === "" || f.level === state.level) &&
      (state.rule === "" || f.id === state.rule) &&
      matchPlace(f, state) &&
      inRange([f.created, f.fn_created, f.event_time], state)
    );
  }

  // Whether an outside-baseline file passes the folder, time and path filters.
  function matchOutside(o, state) {
    return matchPlace(o, state) && inRange([o.created, o.fn_created], state);
  }

  // The rows to draw for a scroll position (ADR 0020). Below the spacer cap a row sits at its
  // own offset; above it (browsers ignore taller elements), the scroll position is mapped onto
  // the rows and the drawn rows follow the viewport. `last` is exclusive.
  function rowRange(scrollTop, viewport, rowHeight, total, cap) {
    const fit = Math.floor(viewport / rowHeight);
    const maxFirst = Math.max(0, total - fit);
    const capped = total * rowHeight > cap;
    const wanted = capped
      ? Math.round((scrollTop / Math.max(1, cap - viewport)) * maxFirst)
      : Math.floor(scrollTop / rowHeight);
    const first = Math.min(Math.max(wanted, 0), maxFirst);
    return { first, last: Math.min(total, first + fit + 1), top: capped ? scrollTop : first * rowHeight };
  }

  // Draws the report into #app and follows the URL hash. Every value is set with textContent:
  // evidence never becomes markup.
  function render(document, window) {
    const data = decode(document.getElementById("data").textContent);

    function el(tag, text, className) {
      const node = document.createElement(tag);
      if (text !== undefined && text !== null) node.textContent = String(text);
      if (className) node.className = className;
      return node;
    }

    const screens = {
      summary: { label: "Summary" },
      findings: { label: "Findings (" + data.findings.length + ")" },
      outside: { label: "Outside-baseline (" + data.outside.length + ")" },
    };
    const app = document.getElementById("app");
    const tabs = app.appendChild(el("nav", undefined, "tabs"));
    for (const [key, screen] of Object.entries(screens)) {
      screen.tab = tabs.appendChild(el("button", screen.label, "tab"));
      screen.tab.setAttribute("data-tab", key);
      screen.tab.addEventListener("click", () => go({ ...state, tab: key }));
      screen.node = app.appendChild(el("section", undefined, "screen"));
      screen.node.setAttribute("data-screen", key);
    }

    let state = parseHash(window.location.hash);
    function go(next) {
      state = next;
      window.location.hash = formatHash(state);
      draw();
    }
    window.addEventListener("hashchange", () => {
      state = parseHash(window.location.hash);
      draw();
    });

    function draw() {
      const shown = state.tab in screens ? state.tab : "summary";
      for (const [key, screen] of Object.entries(screens)) {
        screen.node.style.display = key === shown ? "" : "none";
        screen.tab.className = key === shown ? "tab selected" : "tab";
      }
    }

    summary(screens.summary.node);
    draw();

    function summary(node) {
      const s = data.summary;
      const w = s.window;
      const rows = [
        ["Host", s.host],
        ["Input", s.input + " (" + s.record_size + "-byte records)"],
        ["USN coverage", s.usn_first === null ? "no USN journal" : s.usn_first + " .. " + s.usn_last],
        [
          "Window",
          w === null
            ? "none (all rows listed)"
            : (w.from === null ? "start" : w.from) + " .. " + (w.to === null ? "end" : w.to) + " (" + w.tz + ")",
        ],
        ...(s.warning === null ? [] : [["Warning", s.warning]]),
        ["Findings by level", Object.keys(s.findings).map((k) => k + " " + s.findings[k]).join(", ")],
        ["Outside-baseline files" + (w === null ? "" : " in the window"), s.baseline === null ? "n/a" : s.outside],
        ["Baseline", s.baseline === null ? "none" : s.baseline],
        ["Sigma rules", s.rules + " loaded, " + s.skipped_rules + " skipped (see the skipped-rules CSV/JSONL)"],
        ["MFT entries / files in use", s.entries + " / " + s.files],
        ["Paths resolved / unknown", s.resolved + " / " + s.unknown],
        ["Record diagnostics", Object.keys(s.diagnostics).map((k) => k + " " + s.diagnostics[k]).join(", ")],
      ];
      const table = node.appendChild(el("table", undefined, "summary"));
      for (const [label, value] of rows) {
        const tr = table.appendChild(el("tr"));
        tr.appendChild(el("th", label));
        tr.appendChild(el("td", value));
      }
    }

    const f = data.footer;
    document.getElementById("version").textContent = "tool " + f.version;
    document.getElementById("commit").textContent = f.commit;
    document.getElementById("license").textContent = f.license;
    document.getElementById("source").textContent = f.source_url;
  }

  return { decode, parseHash, formatHash, validTime, matchFinding, matchOutside, rowRange, render };
})();

if (typeof document !== "undefined") viewer.render(document, window);
