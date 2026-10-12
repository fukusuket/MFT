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
    // One row more than fits, so a row cut at the top still leaves the bottom covered.
    const drawn = Math.ceil(viewport / rowHeight) + 1;
    return { first, last: Math.min(total, first + drawn), top: capped ? scrollTop : first * rowHeight };
  }

  const LEVELS = ["critical", "high", "medium", "low", "informational", "none"];
  const FOLDERS = [
    "public", "downloads", "desktop", "documents", "temp", "appdata", "user_profile",
    "programdata", "program_files", "windows", "other", "no_path",
  ];
  const ROW_HEIGHT = 44; // two lines: the row and its facts
  const SPACER_CAP = 8000000; // below Firefox's element height limit (ADR 0020)

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

    const drawn = [];
    function draw() {
      const shown = state.tab in screens ? state.tab : "summary";
      for (const [key, screen] of Object.entries(screens)) {
        screen.node.style.display = key === shown ? "" : "none";
        screen.tab.className = key === shown ? "tab selected" : "tab";
      }
      drawn.forEach((redraw) => redraw());
    }

    // A filter bar; each control writes its key of the state and reads it back on every draw.
    function filters(node, specs) {
      const bar = node.appendChild(el("div", undefined, "filters"));
      const controls = specs.map(([key, label, options]) => {
        const field = bar.appendChild(el("label", label + " "));
        let control;
        if (options) {
          control = field.appendChild(el("select", undefined, "filter-" + key));
          for (const [value, text] of options) {
            const option = control.appendChild(el("option", text));
            option.value = value;
          }
        } else {
          control = field.appendChild(el("input", undefined, "filter-" + key));
          control.setAttribute("placeholder", key === "q" ? "path contains" : "YYYY-MM-DD[THH:MM:SS]");
        }
        control.addEventListener(options ? "change" : "input", () => go({ ...state, [key]: control.value }));
        return [key, control];
      });
      drawn.push(() => {
        for (const [key, control] of controls) {
          // A value from a shared link that this report lacks is offered, so the filter shows.
          if (control.tagName === "SELECT" && !Array.from(control.children).some((o) => o.value === state[key])) {
            const option = control.appendChild(el("option", state[key] + " (not in this report)"));
            option.value = state[key];
          }
          if (control.value !== state[key]) control.value = state[key];
          if (key === "from" || key === "to") {
            control.className = "filter-" + key + (validTime(state[key]) ? "" : " invalid");
          }
        }
      });
    }

    // A virtual-scroll list: only the rows in view exist in the DOM (ADR 0020). `fill` puts
    // one item's content into its fixed-height row.
    function list(node, columns, fill) {
      const head = node.appendChild(el("div", undefined, "list-head"));
      columns.forEach((c) => head.appendChild(el("span", c)));
      const viewport = node.appendChild(el("div", undefined, "viewport"));
      const spacer = viewport.appendChild(el("div", undefined, "spacer"));
      const block = spacer.appendChild(el("div", undefined, "block"));
      let rows = [];
      function paint() {
        const r = rowRange(viewport.scrollTop, viewport.clientHeight, ROW_HEIGHT, rows.length, SPACER_CAP);
        block.style.top = r.top + "px";
        block.replaceChildren(
          ...rows.slice(r.first, r.last).map((item) => {
            const row = el("div", undefined, "row");
            row.style.height = ROW_HEIGHT + "px";
            fill(row, item);
            return row;
          }),
        );
      }
      viewport.addEventListener("scroll", paint);
      return (next) => {
        rows = next;
        spacer.style.height = Math.min(rows.length * ROW_HEIGHT, SPACER_CAP) + "px";
        paint();
      };
    }

    function line(cells) {
      const div = el("div", undefined, "line");
      cells.forEach(([text, className]) => div.appendChild(el("span", text, className)));
      return div;
    }

    summary(screens.summary.node);
    findings(screens.findings.node);
    outside(screens.outside.node);
    draw();

    function outside(node) {
      const per = new Map();
      data.outside.forEach((o) => per.set(o.folder, (per.get(o.folder) || 0) + 1));
      const folders = node.appendChild(el("p", undefined, "folders"));
      FOLDERS.filter((f) => per.has(f)).forEach((folder) => {
        const link = folders.appendChild(el("button", folder + " " + per.get(folder), "folder-link"));
        link.addEventListener("click", () => go({ ...state, folder }));
      });
      filters(node, [
        ["folder", "Folder", [["", "all"], ...FOLDERS.map((f) => [f, f])]],
        ["from", "From"],
        ["to", "To"],
        ["q", "Search"],
      ]);
      const count = node.appendChild(el("p", undefined, "count"));
      const show = list(node, ["Created ($SI, UTC)", "Path", "Folder", "MFT entry"], (row, o) => {
        row.appendChild(line([[o.created, "mono"], [o.path, "path"], [o.folder], [o.entry, "mono"]]));
      });
      drawn.push(() => {
        const shown = data.outside.filter((o) => matchOutside(o, state));
        count.textContent = shown.length + " of " + data.outside.length;
        show(shown);
      });
    }

    function findings(node) {
      const rules = new Map(data.findings.map((f) => [f.id, f.title]));
      filters(node, [
        ["level", "Level", [["", "all"], ...LEVELS.map((l) => [l, l])]],
        ["rule", "Rule", [["", "all"], ...[...rules].sort((a, b) => (a[0] < b[0] ? -1 : 1)).map(([id, t]) => [id, t])]],
        ["folder", "Folder", [["", "all"], ...FOLDERS.map((f) => [f, f])]],
        ["from", "From"],
        ["to", "To"],
        ["q", "Search"],
      ]);
      const count = node.appendChild(el("p", undefined, "count"));
      const show = list(node, ["Level", "Time (UTC)", "Path", "Folder", "Rule"], (row, f) => {
        row.appendChild(
          line([
            [f.level, "level level-" + f.level],
            [f.created !== null ? f.created : f.event_time, "mono"],
            [f.path_state === "resolved" ? f.path : f.path + " (" + f.path_state + ")", "path"],
            [f.folder],
            [f.title + (f.author === null ? " (author unknown)" : " (by " + f.author + ")")], // DRL 1.1
          ]),
        );
        const facts = [
          ...(f.deleted ? ["deleted"] : []),
          ...f.streams.map((s) => "ADS: " + s),
          "seen: " + f.seen,
          ...f.hints,
        ];
        row.appendChild(el("div", facts.join(" · "), "facts"));
        row.addEventListener("click", () => detail(f));
      });
      const pane = node.appendChild(el("div", "Select a row to see why it matched.", "detail"));
      function detail(f) {
        pane.replaceChildren(
          el("h3", f.title),
          el("p", "Rule " + f.id + " · level " + f.level + " · author " + (f.author === null ? "unknown" : f.author)),
          el("p", "MFT entry " + f.entry + (f.usn === null ? "" : " · USN " + f.usn), "mono"),
          el("pre", pretty(f.explain)),
        );
      }
      drawn.push(() => {
        const shown = data.findings.filter((f) => matchFinding(f, state));
        count.textContent = shown.length + " of " + data.findings.length;
        show(shown);
      });
    }

    // The explain trace is JSON; show it indented, or as is if it doesn't parse.
    function pretty(text) {
      try {
        return JSON.stringify(JSON.parse(text), null, 2);
      } catch (e) {
        return text;
      }
    }

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
        ["Findings by level", levelCounts(s.findings)],
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
        const td = tr.appendChild(el("td"));
        if (typeof value === "object") td.appendChild(value);
        else td.textContent = String(value);
      }
    }

    // Each level's count; critical and high open Findings filtered to that level.
    function levelCounts(counts) {
      const cell = el("span");
      LEVELS.forEach((level, i) => {
        if (i > 0) cell.appendChild(el("span", ", "));
        const text = level + " " + counts[level];
        if (level !== "critical" && level !== "high") {
          cell.appendChild(el("span", text));
          return;
        }
        const link = cell.appendChild(el("button", text, "level-link level-" + level));
        link.addEventListener("click", () => go({ ...parseHash(""), tab: "findings", level }));
      });
      return cell;
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
