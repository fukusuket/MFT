//! Thin adapter over the Sigma engine (ADR 0003). Knows nothing about NTFS (ADR 0002 #5).
//!
//! Rules decide; this crate only loads them, routes events by logsource and reports matches.

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use rsigma_eval::Engine;
use rsigma_eval::event::EventValue;
use rsigma_eval::explain::explain_rule;
use rsigma_parser::{Detection, LogSource, SigmaRule, parse_sigma_yaml};

/// Fatal problems loading rules.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("reading rules from {0}: {1}")]
    Io(PathBuf, std::io::Error),
    #[error("rule file {0}: {1}")]
    Rule(PathBuf, String),
}

/// One event to evaluate: its logsource and its fields (standard Sigma field names).
#[derive(Debug, Clone)]
pub struct Event {
    pub product: &'static str,
    pub category: &'static str,
    pub service: Option<&'static str>,
    pub fields: Vec<(&'static str, String)>,
}

/// Sigma `level`, ordered from least to most severe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Informational,
    Low,
    Medium,
    High,
    Critical,
}

/// A rule that matched an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub id: String,
    pub title: String,
    pub level: Option<Level>,
    pub author: Option<String>,
    /// `explain_rule` trace as JSON: which condition, field, matcher and value matched.
    pub explain: String,
}

/// What the caller can emit; a rule that needs anything else could never match and is skipped.
#[derive(Debug, Clone, Copy)]
pub struct Emitted {
    pub product: &'static str,
    pub categories: &'static [&'static str],
    pub services: &'static [&'static str],
    pub fields: &'static [&'static str],
}

/// A rule that was not loaded, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    /// The rule directory's name, then the path below it, joined by `/`.
    pub file: String,
    pub id: Option<String>,
    pub title: Option<String>,
    pub reason: String,
}

/// A loaded rule set.
pub struct Rules {
    engine: Engine,
    ids: Vec<String>,
    /// `author` by rule id; the engine's results don't carry it.
    authors: HashMap<String, Option<String>>,
    /// Position in `engine.rules()` by rule id, for `explain_rule`.
    compiled: HashMap<String, usize>,
    skipped: Vec<Skipped>,
}

impl std::fmt::Debug for Rules {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rules")
            .field("ids", &self.ids)
            .finish_non_exhaustive()
    }
}

impl Rules {
    /// Loads every `.yml`/`.yaml` file under each of `dirs`: the directories in the given order,
    /// the files under each in path order.
    pub fn load(dirs: &[&Path], emitted: &Emitted) -> Result<Self, Error> {
        let mut files = Vec::new();
        for dir in dirs {
            let mut under = Vec::new();
            rule_files(dir, &mut under)?;
            under.sort();
            files.extend(under.into_iter().map(|file| (shown(dir, &file), file)));
        }

        let mut ids = Vec::new();
        let mut authors = HashMap::new();
        let mut loaded = Vec::new();
        // Each with its file's position, so engine errors found later still list in file order.
        let mut skipped = Vec::new();
        for (n, (shown, file)) in files.into_iter().enumerate() {
            let rule_error = |e: &dyn std::fmt::Display| Error::Rule(file.clone(), e.to_string());
            let skip = |id, title, reason: &str| {
                let file = shown.clone();
                let reason = reason.to_string();
                (
                    n,
                    Skipped {
                        file,
                        id,
                        title,
                        reason,
                    },
                )
            };
            let text = std::fs::read_to_string(&file).map_err(|e| Error::Io(file.clone(), e))?;
            let collection = match parse_sigma_yaml(&text) {
                Ok(collection) => collection,
                Err(e) => {
                    skipped.push(skip(None, None, &format!("parse: {e}")));
                    continue;
                }
            };
            // The parser keeps going past bad documents and lists them here.
            for error in &collection.errors {
                skipped.push(skip(None, None, &format!("parse: {error}")));
            }
            // Detection rules only so far; the others are listed, never silently ignored.
            let others = (collection.correlations.into_iter())
                .map(|c| (c.id, c.title, "correlation rule"))
                .chain((collection.filters.into_iter()).map(|f| (f.id, f.title, "filter rule")));
            for (id, title, reason) in others {
                skipped.push(skip(id, Some(title), reason));
            }
            for rule in collection.rules {
                // Detections link to rules by id only, so a rule without one is unusable (ADR 0003).
                let Some(id) = &rule.id else {
                    skipped.push(skip(None, Some(rule.title), "no id"));
                    continue;
                };
                if let Some(reason) = not_emitted(&rule, emitted) {
                    skipped.push(skip(Some(id.clone()), Some(rule.title), &reason));
                    continue;
                }
                if authors.insert(id.clone(), rule.author.clone()).is_some() {
                    return Err(rule_error(&format!(
                        "rule id {id:?} is used more than once"
                    )));
                }
                ids.push(id.clone());
                loaded.push((skip(Some(id.clone()), Some(rule.title.clone()), ""), rule));
            }
        }
        // One batch: each separate add rebuilds the engine's rule index, which is quadratic.
        // The engine keeps the rules it can compile and reports the others.
        let mut engine = Engine::new();
        for (at, e) in engine.add_rules(loaded.iter().map(|(_, rule)| rule)) {
            if let Some(((n, entry), _)) = loaded.get(at) {
                let reason = format!("engine: {e}");
                ids.retain(|id| entry.id.as_ref() != Some(id));
                skipped.push((
                    *n,
                    Skipped {
                        reason,
                        ..entry.clone()
                    },
                ));
            }
        }
        skipped.sort_by_key(|&(n, _)| n);
        let skipped = skipped.into_iter().map(|(_, entry)| entry).collect();
        let compiled = engine
            .rules()
            .iter()
            .enumerate()
            .filter_map(|(i, rule)| Some((rule.id.clone()?, i)))
            .collect();
        Ok(Self {
            engine,
            ids,
            authors,
            compiled,
            skipped,
        })
    }

    /// Rules that were not loaded, in file order.
    pub fn skipped(&self) -> &[Skipped] {
        &self.skipped
    }

    /// Number of loaded rules.
    pub fn count(&self) -> usize {
        self.ids.len()
    }

    /// Rules whose logsource fits the event and whose detection matches it.
    pub fn evaluate(&self, event: &Event) -> Vec<Finding> {
        let logsource = LogSource {
            product: Some(event.product.to_string()),
            category: Some(event.category.to_string()),
            service: event.service.map(str::to_string),
            ..LogSource::default()
        };
        let mut findings: Vec<Finding> = self
            .engine
            .evaluate_with_logsource(event, &logsource)
            .into_iter()
            .filter_map(|result| {
                let id = result.header.rule_id?;
                Some(Finding {
                    author: self.authors.get(&id).cloned().flatten(),
                    title: result.header.rule_title,
                    level: result.header.level.map(level),
                    explain: self.explain(&id, event),
                    id,
                })
            })
            .collect();
        // Most severe first, then by id: the engine's order is not part of its contract (ADR 0002 #3).
        findings.sort_by(|a, b| b.level.cmp(&a.level).then_with(|| a.id.cmp(&b.id)));
        findings
    }

    fn explain(&self, id: &str, event: &Event) -> String {
        self.compiled
            .get(id)
            .and_then(|&i| self.engine.rules().get(i))
            .map(|rule| serde_json::to_string(&explain_rule(rule, event)).unwrap_or_default())
            .unwrap_or_default()
    }
}

/// Why `rule` could never match what is emitted; `None` if it can. A logsource attribute the
/// rule leaves out matches anything.
fn not_emitted(rule: &SigmaRule, emitted: &Emitted) -> Option<String> {
    let source = &rule.logsource;
    let checks = [
        ("product", &source.product, &[emitted.product][..]),
        ("category", &source.category, emitted.categories),
        ("service", &source.service, emitted.services),
    ];
    for (name, value, known) in checks {
        if let Some(value) = value.as_deref()
            && !known.contains(&value)
        {
            return Some(format!("logsource not emitted: {name}={value}"));
        }
    }
    let mut fields = BTreeSet::new();
    for detection in rule.detection.named.values() {
        detection_fields(detection, &mut fields);
    }
    fields.retain(|field| !emitted.fields.contains(&field.as_str()));
    (!fields.is_empty()).then(|| {
        let names: Vec<String> = fields.into_iter().collect();
        format!("field not emitted: {}", names.join(", "))
    })
}

/// Adds the field names `detection` reads to `fields`; keywords read none.
fn detection_fields(detection: &Detection, fields: &mut BTreeSet<String>) {
    match detection {
        Detection::AllOf(items) => {
            fields.extend(items.iter().filter_map(|item| item.field.name.clone()));
        }
        Detection::AnyOf(detections) | Detection::And(detections) => {
            for detection in detections {
                detection_fields(detection, fields);
            }
        }
        Detection::Keywords(_) => {}
        Detection::ArrayMatch { field, .. } => {
            fields.insert(field.clone());
        }
        Detection::Conditional { named, .. } => {
            for detection in named.values() {
                detection_fields(detection, fields);
            }
        }
    }
}

/// How a rule file is named in output: the rule directory's name, then the path below it, joined
/// by `/`. The directories above it are the analyst's and stay out of the report.
fn shown(dir: &Path, file: &Path) -> String {
    let below = file.strip_prefix(dir).unwrap_or(file);
    dir.file_name()
        .into_iter()
        .chain(below.iter())
        .map(|part| part.to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Adds the `.yml`/`.yaml` files under `dir` to `files`. Symbolic links to directories are not
/// followed, so a link loop cannot recurse forever.
fn rule_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), Error> {
    let io = |e| Error::Io(dir.to_path_buf(), e);
    for entry in std::fs::read_dir(dir).map_err(io)? {
        let entry = entry.map_err(io)?;
        let path = entry.path();
        if entry.file_type().map_err(io)?.is_dir() {
            rule_files(&path, files)?;
        } else if path.extension().is_some_and(|x| x == "yml" || x == "yaml") {
            files.push(path);
        }
    }
    Ok(())
}

fn level(level: rsigma_parser::Level) -> Level {
    match level {
        rsigma_parser::Level::Informational => Level::Informational,
        rsigma_parser::Level::Low => Level::Low,
        rsigma_parser::Level::Medium => Level::Medium,
        rsigma_parser::Level::High => Level::High,
        rsigma_parser::Level::Critical => Level::Critical,
    }
}

impl rsigma_eval::event::Event for Event {
    fn get_field(&self, path: &str) -> Option<EventValue<'_>> {
        self.fields
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, value)| EventValue::Str(Cow::Borrowed(value)))
    }

    fn any_string_value(&self, pred: &dyn Fn(&str) -> bool) -> bool {
        self.fields.iter().any(|(_, value)| pred(value))
    }

    fn all_string_values(&self) -> Vec<Cow<'_, str>> {
        self.fields
            .iter()
            .map(|(_, value)| Cow::Borrowed(value.as_str()))
            .collect()
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::Value::Object(
            self.fields
                .iter()
                .map(|(name, value)| ((*name).to_string(), serde_json::Value::from(value.as_str())))
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMITTED: Emitted = Emitted {
        product: "windows",
        categories: &["file_event"],
        services: &["baseline_outside"],
        fields: &["TargetFilename"],
    };

    #[test]
    fn a_rule_for_a_logsource_never_emitted_is_skipped() -> Result<(), Error> {
        let source = |id: &str, from: &str, to: &str| rule(id, id, "").replace(from, to);
        let dir = rule_dir(
            "logsource",
            &[
                (
                    "a.yml",
                    &source(
                        "rule-a",
                        "category: file_event",
                        "category: process_creation",
                    ),
                ),
                (
                    "b.yml",
                    &source("rule-b", "product: windows", "product: linux"),
                ),
                ("c.yml", &rule("rule-c", "C", "  service: security\n")),
                (
                    "d.yml",
                    &rule("rule-d", "D", "  service: baseline_outside\n"),
                ),
                ("e.yml", &source("rule-e", "  category: file_event\n", "")),
            ],
        );

        let rules = Rules::load(&[dir.as_path()], &EMITTED)?;

        let reasons: Vec<&str> = rules.skipped().iter().map(|s| s.reason.as_str()).collect();
        assert_eq!(
            reasons,
            [
                "logsource not emitted: category=process_creation",
                "logsource not emitted: product=linux",
                "logsource not emitted: service=security",
            ]
        );
        assert_eq!(rules.ids, ["rule-d", "rule-e"]);
        Ok(())
    }

    #[test]
    fn a_rule_on_a_field_never_emitted_is_skipped_but_keywords_load() -> Result<(), Error> {
        let image = rule("rule-image", "Image", "").replace(
            "  condition: selection\n",
            "  filter:\n    - Image|endswith: '\\x.exe'\n    - CommandLine: y\n  condition: selection and not filter\n",
        );
        let keywords = "title: K\nid: rule-k\nlogsource:\n  product: windows\ndetection:\n  words:\n    - 'evil'\n  condition: words\n";
        let dir = rule_dir("fields", &[("a.yml", &image), ("b.yml", keywords)]);

        let rules = Rules::load(&[dir.as_path()], &EMITTED)?;

        let reasons: Vec<&str> = rules.skipped().iter().map(|s| s.reason.as_str()).collect();
        assert_eq!(reasons, ["field not emitted: CommandLine, Image"]);
        assert_eq!(rules.ids, ["rule-k"]);
        Ok(())
    }

    #[test]
    fn a_rule_the_engine_cannot_compile_is_skipped_in_file_order() -> Result<(), Error> {
        let bad = rule("rule-bad", "Bad", "").replace("|endswith: '.exe'", "|re: '('");
        let dir = rule_dir(
            "engine",
            &[
                ("a.yml", &bad),
                ("b.yml", &rule("rule-b", "B", "")),
                (
                    "c.yml",
                    &rule("rule-c", "C", "").replace("id: rule-c\n", ""),
                ),
            ],
        );

        let rules = Rules::load(&[dir.as_path()], &EMITTED)?;

        let skipped: Vec<(Option<&str>, bool)> = rules
            .skipped()
            .iter()
            .map(|s| (s.id.as_deref(), s.reason.starts_with("engine: ")))
            .collect();
        assert_eq!(rules.ids, ["rule-b"]);
        assert_eq!(rules.count(), 1);
        assert_eq!(skipped, [(Some("rule-bad"), true), (None, false)]);
        assert_eq!(rules.evaluate(&event(r"C:\x.exe", None)).len(), 1);
        Ok(())
    }

    /// A fresh directory holding the given files.
    pub(crate) fn rule_dir(test: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sigma-tests-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        for (name, text) in files {
            let path = dir.join(name);
            let _ = path.parent().map(std::fs::create_dir_all);
            let _ = std::fs::write(path, text);
        }
        dir
    }

    pub(crate) fn rule(id: &str, title: &str, extra: &str) -> String {
        format!(
            "title: {title}\nid: {id}\nauthor: tool\nlevel: medium\nlogsource:\n  product: windows\n  category: file_event\n{extra}detection:\n  selection:\n    TargetFilename|endswith: '.exe'\n  condition: selection\n"
        )
    }

    #[test]
    fn loads_yml_and_yaml_files_in_name_order() -> Result<(), Error> {
        let dir = rule_dir(
            "order",
            &[
                ("b.yaml", &rule("rule-b", "B", "")),
                ("a.yml", &rule("rule-a", "A", "")),
                ("notes.txt", "not a rule"),
            ],
        );

        let rules = Rules::load(&[dir.as_path()], &EMITTED)?;

        assert_eq!(rules.ids, ["rule-a", "rule-b"]);
        Ok(())
    }

    #[test]
    fn loads_rules_in_subdirectories_in_path_order() -> Result<(), Error> {
        let dir = rule_dir(
            "nested",
            &[
                ("b.yml", &rule("rule-b", "B", "")),
                ("a/deep/c.yml", &rule("rule-c", "C", "")),
                ("a/z.yml", &rule("rule-z", "Z", "")),
            ],
        );

        let rules = Rules::load(&[dir.as_path()], &EMITTED)?;

        assert_eq!(rules.ids, ["rule-c", "rule-z", "rule-b"]);
        Ok(())
    }

    #[test]
    fn loads_several_directories_in_order_and_refuses_an_id_used_in_two() -> Result<(), Error> {
        let first = rule_dir("dirs-1", &[("z.yml", &rule("rule-z", "Z", ""))]);
        let second = rule_dir("dirs-2", &[("a.yml", &rule("rule-a", "A", ""))]);
        let again = rule_dir("dirs-3", &[("b.yml", &rule("rule-z", "Z again", ""))]);

        let rules = Rules::load(&[first.as_path(), second.as_path()], &EMITTED)?;
        let result = Rules::load(&[first.as_path(), again.as_path()], &EMITTED);

        assert_eq!(rules.ids, ["rule-z", "rule-a"]);
        assert!(
            matches!(&result, Err(Error::Rule(file, why)) if file.ends_with("b.yml") && why.contains("rule-z")),
            "{result:?}"
        );
        Ok(())
    }

    #[test]
    fn count_is_the_number_of_loaded_rules() -> Result<(), Error> {
        let dir = rule_dir(
            "len",
            &[
                ("a.yml", &rule("rule-a", "A", "")),
                ("b.yml", &rule("rule-b", "B", "")),
            ],
        );

        assert_eq!(Rules::load(&[dir.as_path()], &EMITTED)?.count(), 2);
        Ok(())
    }

    #[test]
    fn a_rule_without_id_is_skipped_with_its_file_and_reason() -> Result<(), Error> {
        let no_id = rule("x", "No id", "").replace("id: x\n", "");
        let dir = rule_dir(
            "no-id",
            &[("a.yml", &rule("rule-a", "A", "")), ("sub/b.yml", &no_id)],
        );
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        let rules = Rules::load(&[dir.as_path()], &EMITTED)?;

        assert_eq!(rules.ids, ["rule-a"]);
        assert_eq!(
            rules.skipped(),
            [Skipped {
                file: format!("{name}/sub/b.yml"),
                id: None,
                title: Some("No id".to_string()),
                reason: "no id".to_string(),
            }]
        );
        Ok(())
    }

    #[test]
    fn invalid_yaml_or_a_rule_without_detection_is_skipped_as_a_parse_problem() -> Result<(), Error>
    {
        let dir = rule_dir(
            "parse",
            &[
                ("a.yml", &rule("rule-a", "A", "")),
                ("broken.yml", "title: [unclosed\n"),
                (
                    "nodetect.yml",
                    "title: T\nid: t\nlogsource:\n  category: file_event\n",
                ),
            ],
        );

        let rules = Rules::load(&[dir.as_path()], &EMITTED)?;

        let skipped: Vec<(&str, bool)> = rules
            .skipped()
            .iter()
            .map(|s| {
                (
                    s.file.rsplit('/').next().unwrap_or_default(),
                    s.reason.starts_with("parse: "),
                )
            })
            .collect();
        assert_eq!(rules.ids, ["rule-a"]);
        assert_eq!(skipped, [("broken.yml", true), ("nodetect.yml", true)]);
        Ok(())
    }

    #[test]
    fn a_rule_with_an_unknown_modifier_is_skipped_naming_the_modifier() -> Result<(), Error> {
        let odd = rule("rule-odd", "Odd", "").replace("|endswith", "|sometimes");
        let dir = rule_dir(
            "modifier",
            &[("a.yml", &rule("rule-a", "A", "")), ("b.yml", &odd)],
        );

        let rules = Rules::load(&[dir.as_path()], &EMITTED)?;

        let reasons: Vec<&str> = rules.skipped().iter().map(|s| s.reason.as_str()).collect();
        assert_eq!(rules.ids, ["rule-a"]);
        assert!(
            matches!(reasons.as_slice(), [r] if r.starts_with("parse: ") && r.contains("sometimes")),
            "{reasons:?}"
        );
        Ok(())
    }

    fn event(target: &str, service: Option<&'static str>) -> Event {
        Event {
            product: "windows",
            category: "file_event",
            service,
            fields: vec![("TargetFilename", target.to_string())],
        }
    }

    #[test]
    fn a_matching_rule_gives_a_finding_with_id_title_level_and_author() -> Result<(), Error> {
        let dir = rule_dir("finding", &[("a.yml", &rule("rule-a", "Executable", ""))]);
        let rules = Rules::load(&[dir.as_path()], &EMITTED)?;

        let hit = rules.evaluate(&event(r"C:\x.exe", None));
        let miss = rules.evaluate(&event(r"C:\x.txt", None));

        let shown: Vec<_> = hit
            .iter()
            .map(|f| {
                (
                    f.id.as_str(),
                    f.title.as_str(),
                    f.level,
                    f.author.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            shown,
            [("rule-a", "Executable", Some(Level::Medium), Some("tool"))]
        );
        assert!(miss.is_empty());
        Ok(())
    }

    #[test]
    fn baseline_outside_rules_match_only_events_with_that_service() -> Result<(), Error> {
        let outside = rule("rule-out", "Outside", "").replace(
            "  category: file_event\n",
            "  category: file_event\n  service: baseline_outside\n",
        );
        let dir = rule_dir(
            "service",
            &[("a.yml", &rule("rule-any", "Any", "")), ("b.yml", &outside)],
        );
        let rules = Rules::load(&[dir.as_path()], &EMITTED)?;

        let ids = |service| -> Vec<String> {
            let mut ids: Vec<String> = rules
                .evaluate(&event(r"C:\x.exe", service))
                .into_iter()
                .map(|f| f.id)
                .collect();
            ids.sort();
            ids
        };

        assert_eq!(ids(None), ["rule-any"]);
        assert_eq!(ids(Some("baseline_outside")), ["rule-any", "rule-out"]);
        Ok(())
    }

    #[test]
    fn explain_names_the_matched_field_and_value() -> Result<(), Box<dyn std::error::Error>> {
        let dir = rule_dir("explain", &[("a.yml", &rule("rule-a", "Executable", ""))]);
        let rules = Rules::load(&[dir.as_path()], &EMITTED)?;

        let findings = rules.evaluate(&event(r"C:\Windows\x.exe", None));

        let explain: serde_json::Value = serde_json::from_str(
            &findings
                .first()
                .map(|f| f.explain.clone())
                .unwrap_or_default(),
        )?;
        let text = explain.to_string();
        assert!(
            text.contains("TargetFilename")
                && text.contains(r"C:\\Windows\\x.exe")
                && text.contains(".exe"),
            "{text}"
        );
        Ok(())
    }

    #[test]
    fn findings_are_sorted_by_level_then_id() -> Result<(), Error> {
        let at = |id: &str, level: &str| {
            rule(id, id, "").replace("level: medium", &format!("level: {level}"))
        };
        let dir = rule_dir(
            "sorted",
            &[
                ("a.yml", &at("rule-z", "low")),
                ("b.yml", &at("rule-m", "high")),
                ("c.yml", &at("rule-a", "high")),
            ],
        );
        let rules = Rules::load(&[dir.as_path()], &EMITTED)?;

        let ids: Vec<String> = rules
            .evaluate(&event(r"C:\x.exe", None))
            .into_iter()
            .map(|f| f.id)
            .collect();

        assert_eq!(ids, ["rule-a", "rule-m", "rule-z"]);
        Ok(())
    }

    #[test]
    fn a_duplicate_rule_id_is_an_error_naming_the_second_file() {
        let dir = rule_dir(
            "dup",
            &[
                ("a.yml", &rule("same", "A", "")),
                ("b.yml", &rule("same", "B", "")),
            ],
        );

        let result = Rules::load(&[dir.as_path()], &EMITTED);

        assert!(
            matches!(&result, Err(Error::Rule(file, why)) if file.ends_with("b.yml") && why.contains("same")),
            "{result:?}"
        );
    }

    #[test]
    fn correlation_and_filter_documents_are_skipped_but_the_rules_beside_them_load()
    -> Result<(), Error> {
        let correlation = "title: C\nid: c\ncorrelation:\n  type: event_count\n  rules:\n    - rule-a\n  group-by:\n    - TargetFilename\n  timespan: 5m\n  condition:\n    gte: 2\n";
        let filter = "title: F\nid: f\nlogsource:\n  category: file_event\nfilter:\n  rules:\n    - rule-a\n  selection:\n    TargetFilename|endswith: '.tmp'\n  condition: selection\n";
        let doc = format!("{}---\n{correlation}---\n{filter}", rule("rule-a", "A", ""));
        let dir = rule_dir("corr", &[("x.yml", &doc)]);

        let rules = Rules::load(&[dir.as_path()], &EMITTED)?;

        let skipped: Vec<(Option<&str>, Option<&str>, &str)> = rules
            .skipped()
            .iter()
            .map(|s| (s.id.as_deref(), s.title.as_deref(), s.reason.as_str()))
            .collect();
        assert_eq!(rules.ids, ["rule-a"]);
        assert_eq!(
            skipped,
            [
                (Some("c"), Some("C"), "correlation rule"),
                (Some("f"), Some("F"), "filter rule")
            ]
        );
        Ok(())
    }
}
