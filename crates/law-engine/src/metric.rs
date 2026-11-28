use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use law_model::{FixPolicy, Kind, Measure, MetricSpec, Profile, Rule, Safety};

use crate::engine::{FileEngine, Sink};
use crate::fault::Fault;
use crate::outline::Outline;
use crate::relocate::Relocating;
use crate::scope::ScopeMatcher;
use crate::source::Source;

const COMMENT: &str = "comment";
const HASH: char = '#';

/// A `metric` rule with its scope and the profiles of its kinds.
pub struct MetricRule {
    rule: Rule,
    scope: ScopeMatcher,
    measure: Measure,
    over: String,
    profiles: Vec<Profile>,
    moves: Option<Safety>,
}

/// The `metric` engine: each module in the home of a rule's kinds counted for one measure, reported past the
/// threshold of the kind's profile the rule names.
pub struct MetricEngine {
    rules: Vec<MetricRule>,
}

impl MetricRule {
    fn homed<'a>(&'a self, path: &'a Path) -> impl Iterator<Item = &'a Profile> {
        self.profiles
            .iter()
            .filter(move |profile| profile.home.iter().any(|glob| glob.matches(path)))
    }

    fn admits(&self, path: &Path) -> bool {
        self.scope.admits(path) && self.homed(path).next().is_some()
    }

    fn lines(source: &Source<'_>) -> usize {
        let comments: HashSet<usize> = source
            .root
            .root()
            .dfs()
            .filter(|node| node.kind() == COMMENT)
            .map(|node| node.start_pos().line())
            .collect();
        source
            .text
            .lines()
            .enumerate()
            .filter(|(row, line)| {
                let text = line.trim();
                let silent = text.is_empty() || (comments.contains(row) && text.starts_with(HASH));
                !silent
            })
            .count()
    }

    fn count(&self, profile: &Profile, outline: &Outline, source: &Source<'_>) -> usize {
        match self.measure {
            Measure::Lines => Self::lines(source),
            Measure::Classes => outline.classes.len(),
            Measure::Companions => {
                let suffix = profile.suffix.as_deref().unwrap_or_default();
                outline.classes.keys().filter(|name| !name.ends_with(suffix)).count()
            }
            Measure::Functions => outline.defined,
            Measure::Methods => outline
                .classes
                .values()
                .map(|class| class.methods.len())
                .max()
                .unwrap_or_default(),
        }
    }

    fn judge(&self, profile: &Profile, outline: &Outline, source: &Source<'_>, sink: &mut Sink) {
        let Some(limit) = profile.thresholds.get(&self.over) else {
            return;
        };
        let count = self.count(profile, outline, source);
        if count > usize::try_from(*limit).unwrap_or(usize::MAX) {
            let detail = format!("{count} {}, over {limit}", self.measure);
            sink.report(&self.rule, source.path, Sink::origin(), Some(&detail), None);
        }
    }

    fn safety(rule: &Rule, spec: &MetricSpec) -> Result<Option<Safety>, Fault> {
        match (&rule.fix, spec.relocate) {
            (FixPolicy::Safe, Some(_)) => Ok(Some(Safety::Safe)),
            (FixPolicy::Unsafe, Some(_)) => Ok(Some(Safety::Unsafe)),
            (FixPolicy::None { .. }, None) => Ok(None),
            _ => Err(Fault::FixMismatch { rule: rule.id.clone() }),
        }
    }

    /// What moves the modules this rule finds over their measure, when its fix does.
    pub(crate) fn relocating(&self) -> Option<Relocating> {
        self.moves.map(|safety| Relocating {
            rule: self.rule.id.clone(),
            safety,
            profiles: self.profiles.clone(),
        })
    }

    /// A rule measuring its kinds' modules, each kind's profile taken from those handed in.
    ///
    /// # Errors
    /// A fix policy and a `relocate` in the spec that disagree.
    pub fn new(
        rule: &Rule,
        spec: &MetricSpec,
        scope: ScopeMatcher,
        profiles: &BTreeMap<Kind, Profile>,
    ) -> Result<Self, Fault> {
        Ok(Self {
            rule: rule.clone(),
            scope,
            measure: spec.measure,
            over: spec.over.clone(),
            profiles: spec
                .kinds
                .iter()
                .filter_map(|kind| profiles.get(kind))
                .cloned()
                .collect(),
            moves: Self::safety(rule, spec)?,
        })
    }
}

impl MetricEngine {
    /// The engine over its rules.
    #[must_use]
    pub(crate) fn new(rules: Vec<MetricRule>) -> Self {
        Self { rules }
    }
}

impl FileEngine for MetricEngine {
    fn admits(&self, path: &Path) -> bool {
        self.rules.iter().any(|rule| rule.admits(path))
    }

    fn check(&self, source: &Source<'_>, sink: &mut Sink) {
        let rules: Vec<&MetricRule> = self.rules.iter().filter(|rule| rule.admits(source.path)).collect();
        let outline = Outline::new(source.path, &source.root.root());
        sink.census.metric.get_or_insert_default().modules += 1;
        rules
            .iter()
            .flat_map(|rule| rule.homed(source.path).map(move |profile| (*rule, profile)))
            .for_each(|(rule, profile)| rule.judge(profile, &outline, source, sink));
    }
}
