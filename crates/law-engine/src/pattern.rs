use std::collections::HashMap;
use std::error::Error;
use std::iter;
use std::path::Path;

use ast_grep_config::{CombinedScan, GlobalRules, RuleConfig, SerializableRuleConfig};
use ast_grep_core::NodeMatch;
use ast_grep_core::tree_sitter::StrDoc;
use ast_grep_language::SupportLang;
use law_model::{Edit, Fix, FixPolicy, PatternSpec, Rule, Safety};
use serde_json::Value;

use crate::engine::{FileEngine, Sink};
use crate::fault::Fault;
use crate::scope::ScopeMatcher;
use crate::source::Source;

const ID: &str = "id";
const LANGUAGE: &str = "language";
const PYTHON: &str = "python";

/// A `pattern` rule compiled by ast-grep once, with its scope and the safety of its fix.
pub struct PatternRule {
    rule: Rule,
    scope: ScopeMatcher,
    config: RuleConfig<SupportLang>,
    safety: Option<Safety>,
}

/// The `pattern` engine: every admitted rule of a file run in one ast-grep scan.
pub struct PatternEngine {
    rules: Vec<PatternRule>,
    index: HashMap<String, usize>,
}

impl PatternRule {
    fn compile(rule: &Rule, spec: &PatternSpec) -> Result<RuleConfig<SupportLang>, Fault> {
        let bad = |message: String| Fault::BadPattern {
            rule: rule.id.clone(),
            message,
        };
        let mut value = serde_json::to_value(spec).map_err(|error| bad(error.to_string()))?;
        if let Value::Object(map) = &mut value {
            map.insert(ID.to_owned(), Value::from(rule.id.as_str()));
            map.insert(LANGUAGE.to_owned(), Value::from(PYTHON));
        }
        let serializable: SerializableRuleConfig<SupportLang> =
            serde_json::from_value(value).map_err(|error| bad(error.to_string()))?;
        RuleConfig::try_from(serializable, &GlobalRules::default()).map_err(|error| bad(Self::chain(&error)))
    }

    fn chain(error: &(dyn Error + 'static)) -> String {
        iter::successors(Some(error), |&error| error.source())
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(": ")
    }

    fn safety(rule: &Rule, spec: &PatternSpec) -> Result<Option<Safety>, Fault> {
        match (&rule.fix, spec.fix.is_some()) {
            (FixPolicy::Safe, true) => Ok(Some(Safety::Safe)),
            (FixPolicy::Unsafe, true) => Ok(Some(Safety::Unsafe)),
            (FixPolicy::None { .. }, false) => Ok(None),
            _ => Err(Fault::FixMismatch { rule: rule.id.clone() }),
        }
    }

    fn fix(&self, source: &Source<'_>, found: &NodeMatch<'_, StrDoc<SupportLang>>) -> Option<Fix> {
        let safety = self.safety?;
        let edit = found.make_edit(&self.config.matcher, self.config.fixer.first()?);
        Some(Fix {
            safety,
            edits: vec![Edit::Replace {
                path: source.path.to_owned(),
                bytes: edit.position..edit.position + edit.deleted_length,
                text: String::from_utf8_lossy(&edit.inserted_text).into_owned(),
            }],
        })
    }

    /// A rule compiled, its fix policy checked against its spec.
    ///
    /// # Errors
    /// Every fault found: a pattern that does not compile or has no determinable kind, a fix and a policy that disagree.
    pub fn new(rule: &Rule, spec: &PatternSpec, scope: ScopeMatcher) -> Result<Self, Vec<Fault>> {
        match (Self::compile(rule, spec), Self::safety(rule, spec)) {
            (Ok(config), Ok(safety)) => Ok(Self {
                rule: rule.clone(),
                scope,
                config,
                safety,
            }),
            (config, safety) => Err([config.err(), safety.err()].into_iter().flatten().collect()),
        }
    }
}

impl PatternEngine {
    /// The engine over its compiled rules.
    #[must_use]
    pub fn new(rules: Vec<PatternRule>) -> Self {
        let index = rules
            .iter()
            .enumerate()
            .map(|(position, rule)| (rule.config.id.clone(), position))
            .collect();
        Self { rules, index }
    }
}

impl FileEngine for PatternEngine {
    fn admits(&self, path: &Path) -> bool {
        self.rules.iter().any(|rule| rule.scope.admits(path))
    }

    fn check(&self, source: &Source<'_>, sink: &mut Sink) {
        let admitted: Vec<&RuleConfig<SupportLang>> = self
            .rules
            .iter()
            .filter(|rule| rule.scope.admits(source.path))
            .map(|rule| &rule.config)
            .collect();
        let scan = CombinedScan::new(admitted);
        let result = scan.scan(source.root, true);
        let found = result
            .matches
            .into_iter()
            .flat_map(|(config, nodes)| nodes.into_iter().map(move |node| (config, node)))
            .chain(result.diffs);
        found
            .filter_map(|(config, node)| {
                self.index
                    .get(&config.id)
                    .and_then(|&position| self.rules.get(position))
                    .map(|rule| (rule, node))
            })
            .for_each(|(rule, node)| {
                let fix = rule.fix(source, &node);
                sink.report(&rule.rule, source.path, Source::span(node.get_node()), None, fix);
            });
    }
}
