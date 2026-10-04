//! Phase 0 spike S1: can `rsigma-eval` evaluate Sigma rules on NTFS-shaped events?
//! Usage: `cargo run --release -- <dir with SigmaHQ file rules>` (see README.md).

use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::error::Error;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::Instant;

use rsigma_eval::event::{Event, EventValue};
use rsigma_eval::explain::explain_rule;
use rsigma_eval::{CorrelationConfig, CorrelationEngine, Engine, EvaluationResult};
use rsigma_parser::{LogSource, parse_sigma_yaml};
use serde_json::{Value, json};

type Res<T> = Result<T, Box<dyn Error>>;

/// One file-system event as our pipeline would produce it from `$MFT`/`$J`.
struct FileEvent {
    category: &'static str,
    outside_baseline: bool,
    target: String,
    source: Option<String>,
    created: String,
    secs: i64,
}

impl FileEvent {
    fn logsource(&self) -> LogSource {
        LogSource {
            product: Some("windows".into()),
            category: Some(self.category.into()),
            service: self.outside_baseline.then(|| "baseline_outside".into()),
            ..LogSource::default()
        }
    }
}

impl Event for FileEvent {
    fn get_field(&self, path: &str) -> Option<EventValue<'_>> {
        let s = match path {
            "TargetFilename" => self.target.as_str(),
            "SourceFilename" => self.source.as_deref()?,
            "CreationUtcTime" => self.created.as_str(),
            _ => return None,
        };
        Some(EventValue::Str(Cow::Borrowed(s)))
    }

    fn any_string_value(&self, pred: &dyn Fn(&str) -> bool) -> bool {
        pred(&self.target) || self.source.as_deref().is_some_and(pred) || pred(&self.created)
    }

    fn all_string_values(&self) -> Vec<Cow<'_, str>> {
        let mut v = vec![
            Cow::Borrowed(self.target.as_str()),
            Cow::Borrowed(self.created.as_str()),
        ];
        if let Some(s) = &self.source {
            v.push(Cow::Borrowed(s.as_str()));
        }
        v
    }

    fn to_json(&self) -> Value {
        json!({ "TargetFilename": self.target, "SourceFilename": self.source, "CreationUtcTime": self.created })
    }
}

/// Deterministic xorshift so runs are reproducible without a `rand` dependency.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn synthetic_events(n: u64) -> Vec<FileEvent> {
    const BENIGN: [&str; 6] = [
        r"C:\Windows\System32\DriverStore\FileRepository\net{}.inf_amd64\net{}.sys",
        r"C:\Windows\WinSxS\amd64_microsoft-windows-{}_31bf3856ad364e35\f{}.dll",
        r"C:\Users\user\AppData\Local\Microsoft\Edge\User Data\Default\Cache\Cache_Data\f_{}{}",
        r"C:\ProgramData\Microsoft\Windows Defender\Scans\History\Service\{}{}.bin",
        r"C:\Windows\Prefetch\APP{}.EXE-{}.pf",
        r"C:\Users\user\Documents\report{}-{}.docx",
    ];
    const SUSPICIOUS: [&str; 4] = [
        r"C:\PerfLogs\x{}{}.exe",
        r"C:\Users\Public\p{}{}.ps1",
        r"C:\Windows\System32\winevt\Logs\Security{}{}.evtx",
        r"C:\Windows\Temp\{}{}.exe",
    ];
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut out = Vec::new();
    for i in 0..n {
        let r = rng.next();
        let (template, outside) = if i % 1000 == 0 {
            (SUSPICIOUS[usize::try_from(r % 4).unwrap_or(0)], true)
        } else {
            (BENIGN[usize::try_from(r % 6).unwrap_or(0)], false)
        };
        let target =
            template
                .replacen("{}", &(r % 97).to_string(), 1)
                .replacen("{}", &i.to_string(), 1);
        let (category, source) = match r % 10 {
            0..=6 => ("file_event", None),
            7 | 8 => ("file_delete", None),
            _ => ("file_rename", Some(format!("{target}.tmp"))),
        };
        let secs = 1_790_000_000 + i64::try_from(i).unwrap_or(0);
        out.push(FileEvent {
            category,
            outside_baseline: outside,
            target,
            source,
            created: format!("{secs}"),
            secs,
        });
    }
    out
}

fn load_rules(dir: &Path) -> Res<Engine> {
    let mut engine = Engine::new();
    let mut paths: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    paths.sort();
    for p in &paths {
        engine.add_collection(&parse_sigma_yaml(&std::fs::read_to_string(p)?)?)?;
    }
    Ok(engine)
}

fn evaluate_all(engine: &Engine, events: &[FileEvent]) -> Vec<(usize, String)> {
    let mut hits: Vec<(usize, String)> = Vec::new();
    for (i, e) in events.iter().enumerate() {
        for r in engine.evaluate_with_logsource(e, &e.logsource()) {
            hits.push((i, r.header.rule_title.clone()));
        }
    }
    hits.sort();
    hits
}

fn digest(hits: &[(usize, String)]) -> u64 {
    let mut h = DefaultHasher::new();
    hits.hash(&mut h);
    h.finish()
}

fn main() -> Res<()> {
    let dir = std::env::args()
        .nth(1)
        .ok_or("usage: s1-rsigma <rules dir>")?;
    let engine = load_rules(Path::new(&dir))?;
    println!("[load] rules compiled: {}", engine.rule_count());

    // 1. Throughput and determinism.
    let events = synthetic_events(1_000_000);
    let t = Instant::now();
    let a = evaluate_all(&engine, &events);
    let first = t.elapsed();
    let b = evaluate_all(&engine, &events);
    println!(
        "[perf] {} rules x {} events: {:.2?} ({} matches)",
        engine.rule_count(),
        events.len(),
        first,
        a.len()
    );
    println!(
        "[determinism] run1 {:016x} run2 {:016x} equal={}",
        digest(&a),
        digest(&b),
        a == b
    );
    let mut by_rule: Vec<(String, usize)> = Vec::new();
    for (_, title) in &a {
        match by_rule.iter_mut().find(|(t, _)| t == title) {
            Some((_, n)) => *n += 1,
            None => by_rule.push((title.clone(), 1)),
        }
    }
    by_rule.sort_by(|x, y| y.1.cmp(&x.1).then(x.0.cmp(&y.0)));
    for (title, n) in by_rule.iter().take(6) {
        println!("         {n:>6}  {title}");
    }

    // 2. explain: is the trace usable as finding evidence?
    if let Some((i, title)) = a.first() {
        if let (Some(rule), Some(ev)) = (
            engine.rules().iter().find(|r| &r.title == title),
            events.get(*i),
        ) {
            println!("[explain] event {:?} ({})", ev.target, ev.category);
            println!("{}", serde_json::to_string_pretty(&explain_rule(rule, ev))?);
        }
        if let Some(ev) = events.get(*i) {
            let details: Vec<EvaluationResult> =
                engine.evaluate_with_logsource(ev, &ev.logsource());
            if let Some(d) = details.first().and_then(|r| r.as_detection()) {
                println!(
                    "[evidence] matched_selections={:?} matched_fields={:?}",
                    d.matched_selections, d.matched_fields
                );
            }
        }
    }

    routing()?;
    correlation()?;
    Ok(())
}

/// 3. `service: baseline_outside` routes rules to outside-baseline events only.
fn routing() -> Res<()> {
    let mut engine = Engine::new();
    engine.add_collection(&parse_sigma_yaml(
        r"
title: Outside-baseline executable
logsource: { product: windows, category: file_event, service: baseline_outside }
detection:
  sel:
    TargetFilename|endswith: '.exe'
  condition: sel
level: medium
---
title: Any executable (no service)
logsource: { product: windows, category: file_event }
detection:
  sel:
    TargetFilename|endswith: '.exe'
  condition: sel
level: low
",
    )?)?;
    for outside in [false, true] {
        let ev = FileEvent {
            category: "file_event",
            outside_baseline: outside,
            target: r"C:\ProgramData\x.exe".into(),
            source: None,
            created: "0".into(),
            secs: 0,
        };
        let titles: Vec<String> = engine
            .evaluate_with_logsource(&ev, &ev.logsource())
            .into_iter()
            .map(|r| r.header.rule_title)
            .collect();
        println!("[routing] outside_baseline={outside}: {titles:?}");
    }
    Ok(())
}

/// 4. `temporal_ordered`: executable created then deleted within 5 minutes (USN times).
fn correlation() -> Res<()> {
    let collection = parse_sigma_yaml(
        r"
title: Executable created
id: 0b6a6f2e-1c1e-4d8e-9a51-6c3f0a000001
name: exe_created
logsource: { product: windows, category: file_event }
detection:
  sel:
    TargetFilename|endswith: '.exe'
  condition: sel
level: informational
---
title: Executable deleted
id: 0b6a6f2e-1c1e-4d8e-9a51-6c3f0a000002
name: exe_deleted
logsource: { product: windows, category: file_delete }
detection:
  sel:
    TargetFilename|endswith: '.exe'
  condition: sel
level: informational
---
title: Executable created then deleted
correlation:
  type: temporal_ordered
  rules: [exe_created, exe_deleted]
  group-by: [TargetFilename]
  timespan: 5m
level: high
",
    )?;
    let mut ce = CorrelationEngine::new(CorrelationConfig {
        timestamp_fallback: rsigma_eval::correlation_engine::TimestampFallback::Skip,
        ..CorrelationConfig::default()
    });
    ce.add_collection(&collection)?;
    let ev = |category: &'static str, target: &str, secs: i64| FileEvent {
        category,
        outside_baseline: false,
        target: target.into(),
        source: None,
        created: secs.to_string(),
        secs,
    };
    let seq = [
        ev("file_event", r"C:\Windows\Temp\a.exe", 1000), // create a
        ev("file_delete", r"C:\Windows\Temp\a.exe", 1060), // delete a after 60 s -> should fire
        ev("file_event", r"C:\Windows\Temp\b.exe", 2000), // create b
        ev("file_delete", r"C:\Windows\Temp\b.exe", 5600), // delete b after 1 h -> must not fire
        ev("file_delete", r"C:\Windows\Temp\c.exe", 6000), // delete without create -> must not fire
        ev("file_delete", r"C:\Windows\Temp\d.exe", 7000), // delete before create (wrong order)
        ev("file_event", r"C:\Windows\Temp\d.exe", 7010), //  -> must not fire
    ];
    for e in &seq {
        let detections = ce.engine().evaluate_with_logsource(e, &e.logsource());
        let fired: Vec<String> = ce
            .process_with_detections(e, detections, e.secs)
            .into_iter()
            .filter(|r| r.is_correlation())
            .map(|r| r.header.rule_title)
            .collect();
        println!(
            "[correlation] t={:>5} {:<11} {:<24} fired={fired:?}",
            e.secs, e.category, e.target
        );
    }
    Ok(())
}
