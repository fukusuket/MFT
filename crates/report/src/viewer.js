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
  const KEYS = ["tab", "level", "rule", "folder", "state", "from", "to", "q"];

  function parseHash(hash) {
    const state = { tab: "summary", level: "", rule: "", folder: "", state: "", from: "", to: "", q: "" };
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

  // A finding's facts as shown under its row.
  function facts(f) {
    return [...f.streams.map((s) => "ADS: " + s), "seen: " + f.seen, ...f.hints];
  }

  // The lowercased text of every column a row shows, for the search. The state is left out:
  // its drop-down filters it.
  function joined(columns) {
    return columns.filter((c) => c !== null && c !== undefined).join("\n").toLowerCase();
  }
  function findingText(f) {
    return joined([f.level, f.created, f.fn_created, f.event_time, f.path, f.folder, f.title, f.id, f.author, ...facts(f)]);
  }
  function outsideText(o) {
    return joined([o.created, o.fn_created, o.path, String(o.entry), o.folder]);
  }

  // A file's state cell, text and class: deleted or present.
  function fileState(file) {
    return file.deleted ? ["deleted", "state state-deleted"] : ["present", "state"];
  }

  // Whether a finding passes every filter that is set. The search is case-insensitive over the
  // shown columns; `text` is the row's search text when the caller keeps it (lists do).
  function matchFinding(f, state, text) {
    return (
      (state.level === "" || f.level === state.level) &&
      (state.rule === "" || f.id === state.rule) &&
      (state.folder === "" || f.folder === state.folder) &&
      (state.state === "" || fileState(f)[0] === state.state) &&
      (state.q === "" || (text ?? findingText(f)).includes(state.q.toLowerCase())) &&
      inRange([f.created, f.fn_created, f.event_time], state)
    );
  }

  // Whether an outside-baseline file passes the folder, state, time and search filters.
  function matchOutside(o, state, text) {
    return (
      (state.folder === "" || o.folder === state.folder) &&
      (state.state === "" || fileState(o)[0] === state.state) &&
      (state.q === "" || (text ?? outsideText(o)).includes(state.q.toLowerCase())) &&
      inRange([o.created, o.fn_created], state)
    );
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
  const STATES = ["present", "deleted"];
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
      screen.redraws = [];
    }

    let state = parseHash(window.location.hash);
    // A tab change is a history entry, so Back returns to the last screen; a filter change
    // only replaces it, so typing a search does not fill the history.
    function go(next) {
      const tabChanged = next.tab !== state.tab;
      state = next;
      if (tabChanged) window.location.hash = formatHash(state);
      else window.history.replaceState(null, "", formatHash(state));
      draw();
    }
    window.addEventListener("hashchange", () => {
      const next = parseHash(window.location.hash);
      if (formatHash(next) === formatHash(state)) return; // the hash `go` just set
      state = next;
      draw();
    });

    // Shows the chosen screen and redraws only it: filtering is the costly part.
    function draw() {
      const shown = state.tab in screens ? state.tab : "summary";
      for (const [key, screen] of Object.entries(screens)) {
        screen.node.style.display = key === shown ? "" : "none";
        screen.tab.className = key === shown ? "tab selected" : "tab";
      }
      screens[shown].redraws.forEach((redraw) => redraw());
    }

    // A list screen's filter bar, most used first: search and time range, then the level, rule,
    // folder and state drop-downs with their counts in `rows`, the count node it returns, and
    // Clear. Each control writes its key of the state and reads it back on every draw.
    function filters(screen, rows, keys) {
      const bar = screen.node.appendChild(el("div", undefined, "filters"));
      const inputs = [
        ["q", "Search", "any column contains"],
        ["from", "From", "YYYY-MM-DD[THH:MM:SS]"],
        ["to", "To", "YYYY-MM-DD[THH:MM:SS]"],
      ].map(([key, label, hint]) => {
        const input = bar.appendChild(el("label", label + " ")).appendChild(el("input", undefined, "filter-" + key));
        input.setAttribute("placeholder", hint);
        input.addEventListener("input", () => go({ ...state, [key]: input.value }));
        return [key, input];
      });
      // Each drop-down's options: all, then the values in `rows` with their counts.
      const counted = (values, of) => {
        const counts = new Map();
        rows.map(of).forEach((v) => counts.set(v, (counts.get(v) || 0) + 1));
        return values.filter((v) => counts.has(v)).map((v) => [v, v + " (" + counts.get(v) + ")"]);
      };
      // Rules by level as in LEVELS, then title, then id, so the order is total.
      const ruleOptions = () => {
        const rules = new Map(); // id → its level, title and row count
        rows.forEach((r) => {
          const seen = rules.get(r.id) || { level: r.level, title: r.title, n: 0 };
          rules.set(r.id, { ...seen, n: seen.n + 1 });
        });
        const compare = (x, y) => (x < y ? -1 : x > y ? 1 : 0);
        const order = ([ia, a], [ib, b]) =>
          LEVELS.indexOf(a.level) - LEVELS.indexOf(b.level) || compare(a.title, b.title) || compare(ia, ib);
        return [...rules].sort(order).map(([id, r]) => [id, r.title + " (" + r.n + ")"]);
      };
      const selects = [
        ["level", "Level", () => counted(LEVELS, (r) => r.level)],
        ["rule", "Rule", ruleOptions],
        ["folder", "Folder", () => counted(FOLDERS, (r) => r.folder)],
        ["state", "State", () => counted(STATES, (r) => fileState(r)[0])],
      ]
        .filter(([key]) => keys.includes(key))
        .map(([key, title, options]) => {
          const select = bar.appendChild(el("label", title + " ")).appendChild(el("select", undefined, "filter-" + key));
          for (const [value, label] of [["", "all " + key + "s"], ...options()]) select.appendChild(el("option", label)).value = value;
          select.addEventListener("change", () => go({ ...state, [key]: select.value }));
          return [key, select];
        });
      const count = bar.appendChild(el("span", undefined, "count"));
      bar.appendChild(el("button", "Clear", "clear")).addEventListener("click", () =>
        go({ ...parseHash(""), tab: state.tab }),
      );

      screen.redraws.push(() => {
        for (const [key, input] of inputs) {
          if (input.value !== state[key]) input.value = state[key];
          if (key !== "q") input.className = "filter-" + key + (validTime(state[key]) ? "" : " invalid");
        }
        for (const [key, select] of selects) {
          // A value from a shared link that this report lacks is offered, so the filter shows.
          if (!Array.from(select.children).some((o) => o.value === state[key])) {
            select.appendChild(el("option", state[key] + " (not in this report)")).value = state[key];
          }
          if (select.value !== state[key]) select.value = state[key];
        }
      });
      return count;
    }

    // A virtual-scroll list: only the rows in view exist in the DOM (ADR 0020). `fill` puts
    // one item's content into its fixed-height row. A click selects a row, which stays
    // highlighted while the list repaints, and shows `detail(item)` in the pane below it,
    // scrolled into view; `hint` fills the pane until then.
    function list(node, columns, fill, detail, hint) {
      const head = node.appendChild(el("div", undefined, "list-head"));
      columns.forEach((c) => head.appendChild(el("span", c)));
      const viewport = node.appendChild(el("div", undefined, "viewport"));
      const spacer = viewport.appendChild(el("div", undefined, "spacer"));
      const block = spacer.appendChild(el("div", undefined, "block"));
      const pane = node.appendChild(el("div", hint, "detail"));
      let rows = [];
      let selected = null;
      function paint() {
        const r = rowRange(viewport.scrollTop, viewport.clientHeight, ROW_HEIGHT, rows.length, SPACER_CAP);
        block.style.top = r.top + "px";
        block.replaceChildren(
          ...rows.slice(r.first, r.last).map((item) => {
            const row = el("div", undefined, item === selected ? "row selected" : "row");
            row.style.height = ROW_HEIGHT + "px";
            fill(row, item);
            row.addEventListener("click", () => {
              selected = item;
              paint();
              pane.replaceChildren(...detail(item));
              pane.scrollIntoView({ block: "nearest" });
            });
            return row;
          }),
        );
      }
      viewport.addEventListener("scroll", paint);
      window.addEventListener("resize", paint); // the list fills the window, so its height follows
      return (next) => {
        rows = next;
        spacer.style.height = Math.min(rows.length * ROW_HEIGHT, SPACER_CAP) + "px";
        paint();
      };
    }

    // One row's cells, each with its full text as a tooltip, since a cell that does not fit is
    // cut. A path loses its start, so the file name stays.
    function line(cells) {
      const div = el("div", undefined, "line");
      cells.forEach(([text, className]) => {
        const path = className === "path";
        const cell = div.appendChild(el("span", path ? undefined : text, className));
        cell.setAttribute("title", text ?? ""); // as shown: `el` leaves a missing value empty
        if (path) cell.appendChild(el("span", text)).setAttribute("dir", "ltr");
      });
      return div;
    }

    summary(screens.summary.node);
    findings(screens.findings);
    outside(screens.outside);
    draw();

    function outside(screen) {
      const node = screen.node;
      const count = filters(screen, data.outside, ["folder", "state"]);
      const texts = data.outside.map(outsideText); // built at load, so no search pays for it
      const show = list(
        node,
        ["Created ($SI, UTC)", "State", "Path"],
        (row, o) => row.appendChild(line([[o.created, "mono"], fileState(o), [o.path, "path"]])),
        (o) => [
          el("p", o.path, "path"),
          el("p", "$SI created " + (o.created ?? "unknown") + " · $FN created " + (o.fn_created ?? "unknown"), "mono"),
          el("p", "MFT entry " + o.entry + " · folder " + o.folder),
        ],
        "Select a row to see its full path.",
      );
      screen.redraws.push(() => {
        const shown = data.outside.filter((o, i) => matchOutside(o, state, texts[i]));
        count.textContent = shown.length + " of " + data.outside.length;
        show(shown);
      });
    }

    function findings(screen) {
      const node = screen.node;
      const count = filters(screen, data.findings, ["level", "rule", "folder", "state"]);
      const texts = data.findings.map(findingText); // built at load, so no search pays for it
      const show = list(node, ["Level", "State", "Time (UTC)", "Path", "Folder", "Rule"], (row, f) => {
        row.appendChild(
          line([
            [f.level, "level level-" + f.level],
            fileState(f),
            [f.created !== null ? f.created : f.event_time, "mono"],
            [f.path_state === "resolved" ? f.path : f.path + " (" + f.path_state + ")", "path"],
            [f.folder],
            [f.title + (f.author === null ? " (author unknown)" : " (by " + f.author + ")")], // DRL 1.1
          ]),
        );
        row.appendChild(el("div", facts(f).join(" · "), "facts"));
      }, (f) => [
        el("h3", f.title),
        el("p", f.path_state === "resolved" ? f.path : f.path + " (" + f.path_state + ")", "path"),
        el("p", "Rule " + f.id + " · level " + f.level + " · author " + (f.author === null ? "unknown" : f.author)),
        el("pre", pretty(f.explain)),
      ], "Select a row to see why it matched.");
      screen.redraws.push(() => {
        const shown = data.findings.filter((f, i) => matchFinding(f, state, texts[i]));
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
        ["Edition", s.edition === undefined ? "not found" : s.edition],
        ...(s.baseline_warning === undefined ? [] : [["Baseline warning", s.baseline_warning]]),
        ["Input", s.input],
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
