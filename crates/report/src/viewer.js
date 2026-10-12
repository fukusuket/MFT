"use strict";
(function () {
  const raw = document.getElementById("data").textContent.trim();
  const bytes = Uint8Array.from(atob(raw), (c) => c.charCodeAt(0));
  const data = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));

  // Every value is set with textContent: evidence never becomes markup.
  function el(tag, text, className) {
    const node = document.createElement(tag);
    if (text !== undefined && text !== null) node.textContent = String(text);
    if (className) node.className = className;
    return node;
  }

  // The explain trace is JSON; show it indented, or as is if it doesn't parse.
  function pretty(text) {
    try {
      return JSON.stringify(JSON.parse(text), null, 2);
    } catch (e) {
      return text;
    }
  }

  function table(headers, rows) {
    if (rows.length === 0) return el("p", "None.", "muted");
    const t = el("table");
    const head = el("tr");
    headers.forEach((h) => head.appendChild(el("th", h)));
    t.appendChild(el("thead")).appendChild(head);
    const body = el("tbody");
    rows.forEach((cells) => {
      const tr = el("tr");
      cells.forEach((cell) => tr.appendChild(cell));
      body.appendChild(tr);
    });
    t.appendChild(body);
    return t;
  }

  const s = data.summary;
  const levels = Object.keys(s.findings).map((k) => k + " " + s.findings[k]).join(", ");
  const diagnostics = Object.keys(s.diagnostics).map((k) => k + " " + s.diagnostics[k]).join(", ");
  const w = s.window;
  const range = w === null
    ? "none (all rows listed)"
    : (w.from === null ? "start" : w.from) + " .. " + (w.to === null ? "end" : w.to) + " (" + w.tz + ")";
  const summary = document.getElementById("summary");
  [
    ["Time window (findings and outside-baseline files listed and counted in it)", range],
    ...(s.warning === null ? [] : [["Warning", s.warning]]),
    ["Host", s.host],
    ["Input", s.input],
    ["MFT record size", s.record_size],
    ["MFT entries", s.entries],
    ["Files (in use)", s.files],
    ["Paths resolved / unknown", s.resolved + " / " + s.unknown],
    ["Record diagnostics", diagnostics],
    ["Baseline", s.baseline === null ? "none" : s.baseline],
    ["Outside-baseline files", s.baseline === null ? "n/a" : s.outside],
    ["Sigma rules loaded", s.rules],
    ["Sigma rules skipped (see the skipped-rules CSV/JSONL)", s.skipped_rules],
    ["Findings by level", levels],
  ].forEach(([label, value]) => {
    const tr = el("tr");
    tr.appendChild(el("th", label));
    tr.appendChild(el("td", value));
    summary.appendChild(tr);
  });

  // Shown next to a finding, never used to order it. One line per fact and per stream, since a
  // stream name may contain any separator.
  function facts(f) {
    const cell = el("td", undefined, "path");
    const lines = [
      ...(f.deleted ? ["deleted"] : []),
      "seen: " + f.seen,
      ...f.streams.map((s) => "ADS: " + s),
      ...f.hints,
    ];
    lines.forEach((line) => cell.appendChild(el("div", line)));
    return cell;
  }

  document.getElementById("findings").appendChild(table(
    ["Level", "Time ($SI created / USN)", "USN", "Path", "Facts", "Rule", "Author", "Rule id"],
    data.findings.map((f) => {
      const rule = el("td");
      rule.appendChild(el("div", f.title));
      const details = el("details");
      details.appendChild(el("summary", "Why it matched"));
      details.appendChild(el("pre", pretty(f.explain)));
      rule.appendChild(details);
      return [
        el("td", f.level, "level level-" + f.level),
        el("td", f.created !== null ? f.created : f.event_time, "mono"),
        el("td", f.usn, "mono"),
        el("td", f.path_state === "resolved" ? f.path : f.path + " (" + f.path_state + ")", "path"),
        facts(f),
        rule,
        el("td", f.author === null ? "unknown" : f.author),
        el("td", f.id, "mono"),
      ];
    })
  ));

  document.getElementById("outside").appendChild(table(
    ["Created ($SI)", "Path", "MFT entry"],
    data.outside.map((o) => [el("td", o.created, "mono"), el("td", o.path, "path"), el("td", o.entry, "mono")])
  ));

  const f = data.footer;
  document.getElementById("version").textContent = "tool " + f.version;
  document.getElementById("commit").textContent = f.commit;
  document.getElementById("license").textContent = f.license;
  document.getElementById("source").textContent = f.source_url;
})();
