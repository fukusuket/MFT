//! Thin adapter over the Sigma engine (ADR 0003). Knows nothing about NTFS (ADR 0002 #5).
//!
//! Rules decide; this crate only loads them, routes events by logsource and reports matches.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rsigma_eval::Engine;
use rsigma_eval::event::EventValue;
use rsigma_eval::explain::explain_rule;
use rsigma_parser::{LogSource, parse_sigma_yaml};

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

/// A loaded rule set.
pub struct Rules {
    engine: Engine,
    ids: Vec<String>,
    /// `author` by rule id; the engine's results don't carry it.
    authors: HashMap<String, Option<String>>,
    /// Position in `engine.rules()` by rule id, for `explain_rule`.
    compiled: HashMap<String, usize>,
}

impl std::fmt::Debug for Rules {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rules")
            .field("ids", &self.ids)
            .finish_non_exhaustive()
    }
}

impl Rules {
    /// Loads every `.yml`/`.yaml` file in `dir`, in file-name order.
    pub fn load(dir: &Path) -> Result<Self, Error> {
        let io = |e| Error::Io(dir.to_path_buf(), e);
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
            .map_err(io)?
            .map(|entry| entry.map(|e| e.path()))
            .collect::<Result<_, _>>()
            .map_err(io)?;
        files.retain(|f| f.extension().is_some_and(|x| x == "yml" || x == "yaml"));
        files.sort();

        let mut ids = Vec::new();
        let mut authors = HashMap::new();
        let mut loaded = Vec::new();
        for file in files {
            let rule_error = |e: &dyn std::fmt::Display| Error::Rule(file.clone(), e.to_string());
            let text = std::fs::read_to_string(&file).map_err(|e| Error::Io(file.clone(), e))?;
            let collection = parse_sigma_yaml(&text).map_err(|e| rule_error(&e))?;
            // The parser keeps going past bad documents and lists them here.
            if let Some(first) = collection.errors.first() {
                return Err(rule_error(first));
            }
            // Phase 1 runs detection rules only; refuse what would otherwise be silently ignored.
            if !collection.correlations.is_empty() || !collection.filters.is_empty() {
                return Err(rule_error(
                    &"correlation and filter rules are not supported yet",
                ));
            }
            for rule in &collection.rules {
                // Detections link to rules by id only, so a rule without one is unusable (ADR 0003).
                let Some(id) = &rule.id else {
                    return Err(rule_error(&format!("rule {:?} has no id", rule.title)));
                };
                if authors.insert(id.clone(), rule.author.clone()).is_some() {
                    return Err(rule_error(&format!(
                        "rule id {id:?} is used more than once"
                    )));
                }
                ids.push(id.clone());
            }
            loaded.extend(
                collection
                    .rules
                    .into_iter()
                    .map(|rule| (file.clone(), rule)),
            );
        }
        // One batch: each separate add rebuilds the engine's rule index, which is quadratic.
        let mut engine = Engine::new();
        if let Some((at, e)) = engine
            .add_rules(loaded.iter().map(|(_, rule)| rule))
            .first()
        {
            let file = loaded
                .get(*at)
                .map(|(file, _)| file.clone())
                .unwrap_or_default();
            return Err(Error::Rule(file, e.to_string()));
        }
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
        })
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

    /// A fresh directory holding the given files.
    pub(crate) fn rule_dir(test: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sigma-tests-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        for (name, text) in files {
            let _ = std::fs::write(dir.join(name), text);
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

        let rules = Rules::load(&dir)?;

        assert_eq!(rules.ids, ["rule-a", "rule-b"]);
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

        assert_eq!(Rules::load(&dir)?.count(), 2);
        Ok(())
    }

    #[test]
    fn a_rule_without_id_is_an_error_naming_the_file() {
        let no_id = rule("x", "No id", "").replace("id: x\n", "");
        let dir = rule_dir(
            "no-id",
            &[("a.yml", &rule("rule-a", "A", "")), ("b.yml", &no_id)],
        );

        let result = Rules::load(&dir);

        assert!(
            matches!(&result, Err(Error::Rule(file, why)) if file.ends_with("b.yml") && why.contains("no id")),
            "{result:?}"
        );
    }

    #[test]
    fn invalid_yaml_or_a_rule_without_detection_is_an_error_naming_the_file() {
        let cases = [
            ("broken.yml", "title: [unclosed\n"),
            (
                "nodetect.yml",
                "title: T\nid: t\nlogsource:\n  category: file_event\n",
            ),
        ];
        for (name, text) in cases {
            let dir = rule_dir(name, &[(name, text)]);

            let result = Rules::load(&dir);

            assert!(
                matches!(&result, Err(Error::Rule(file, _)) if file.ends_with(name)),
                "{name}: {result:?}"
            );
        }
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
        let rules = Rules::load(&dir)?;

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
        let rules = Rules::load(&dir)?;

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
        let rules = Rules::load(&dir)?;

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
        let rules = Rules::load(&dir)?;

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

        let result = Rules::load(&dir);

        assert!(
            matches!(&result, Err(Error::Rule(file, why)) if file.ends_with("b.yml") && why.contains("same")),
            "{result:?}"
        );
    }

    #[test]
    fn correlation_and_filter_documents_are_errors_until_supported() {
        let correlation = "title: C\nid: c\ncorrelation:\n  type: event_count\n  rules:\n    - rule-a\n  group-by:\n    - TargetFilename\n  timespan: 5m\n  condition:\n    gte: 2\n";
        let filter = "title: F\nid: f\nlogsource:\n  category: file_event\nfilter:\n  rules:\n    - rule-a\n  selection:\n    TargetFilename|endswith: '.tmp'\n  condition: selection\n";
        for (name, text) in [("corr.yml", correlation), ("filter.yml", filter)] {
            let doc = format!("{}---\n{text}", rule("rule-a", "A", ""));
            let dir = rule_dir(name, &[(name, &doc)]);

            let result = Rules::load(&dir);

            assert!(
                matches!(&result, Err(Error::Rule(file, _)) if file.ends_with(name)),
                "{name}: {result:?}"
            );
        }
    }
}
