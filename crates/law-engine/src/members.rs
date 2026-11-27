use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use law_model::{MembersSpec, Order, Rule};

use crate::ancestry::{Ancestor, Ancestry};
use crate::engine::{FileEngine, Sink};
use crate::outline::{Class, Method, Outline};
use crate::scope::ScopeMatcher;
use crate::source::Source;

const UNDERSCORE: char = '_';
const DUNDER: &str = "__";
const ASYNC: char = 'a';

/// A `members` rule with its scope.
pub struct MembersRule {
    rule: Rule,
    scope: ScopeMatcher,
    spec: MembersSpec,
}

/// The `members` engine: each class's methods read off the parse — the lineage of its privates, their order — its
/// bases resolved through the project's modules and its environment's, read and never imported; a class whose line
/// does not wholly resolve is not judged at all, for a method it overrides may be a foreign base's.
pub struct MembersEngine {
    rules: Vec<MembersRule>,
    ancestry: Arc<Ancestry>,
}

struct Name;

impl Name {
    fn special(name: &str, publics: &BTreeSet<&str>) -> bool {
        let core = name.trim_matches(UNDERSCORE);
        let sunder = !name.starts_with(DUNDER) && publics.contains(core);
        name.starts_with(UNDERSCORE) && name.ends_with(UNDERSCORE) && !core.is_empty() && !sunder
    }

    fn private(name: &str, publics: &BTreeSet<&str>) -> bool {
        name.starts_with(UNDERSCORE) && !Self::special(name, publics)
    }

    fn publics(class: &Class) -> impl Iterator<Item = &str> {
        class.methods.iter().flat_map(|method| {
            let name = method.name.as_str();
            let dunder = name.len() > 2 * DUNDER.len() && name.starts_with(DUNDER) && name.ends_with(DUNDER);
            let stem = dunder.then(|| name.trim_matches(UNDERSCORE));
            let own = (!name.starts_with(UNDERSCORE)).then_some(name);
            let awaited = stem.and_then(|stem| stem.strip_prefix(ASYNC));
            own.into_iter().chain(stem).chain(awaited)
        })
    }

    fn serves<'a>(name: &str, stems: impl IntoIterator<Item = &'a str>) -> bool {
        let core = name.trim_start_matches(UNDERSCORE);
        stems.into_iter().any(|stem| {
            core.strip_prefix(stem)
                .and_then(|rest| rest.strip_prefix(UNDERSCORE))
                .is_some_and(|what| !what.trim_matches(UNDERSCORE).is_empty())
        })
    }
}

impl MembersRule {
    fn judged<'a>(&self, class: &'a Class, ancestors: &[Ancestor], publics: &BTreeSet<&str>) -> Vec<&'a Method> {
        let foreign: BTreeSet<&str> = ancestors
            .iter()
            .filter(|ancestor| ancestor.outline.foreign.is_some() || !self.scope.admits(&ancestor.outline.path))
            .flat_map(|ancestor| ancestor.class.methods.iter().map(|method| method.name.as_str()))
            .collect();
        let mut seen = BTreeSet::new();
        class
            .methods
            .iter()
            .filter(|method| seen.insert(method.name.as_str()))
            .filter(|method| !Name::special(&method.name, publics) && !foreign.contains(method.name.as_str()))
            .filter(|method| {
                !method
                    .decorators
                    .iter()
                    .any(|decorator| self.spec.hooks.contains(decorator))
            })
            .collect()
    }

    fn lineage(&self, path: &Path, class: &Class, methods: &[&Method], publics: &BTreeSet<&str>, sink: &mut Sink) {
        let Some(lineage) = &self.spec.lineage else {
            return;
        };
        let stems = || publics.iter().copied().chain(lineage.common.iter().map(String::as_str));
        methods
            .iter()
            .filter(|method| Name::private(&method.name, publics) && !Name::serves(&method.name, stems()))
            .for_each(|method| {
                let detail = format!("{} serves no public of {}", method.name, class.name);
                sink.report(&self.rule, path, method.span.clone(), Some(&detail), None);
            });
    }

    fn order(&self, path: &Path, methods: &[&Method], publics: &BTreeSet<&str>, sink: &mut Sink) {
        let Some(Order::PrivatesFirst) = self.spec.order else {
            return;
        };
        let Some(at) = methods.iter().position(|method| !method.name.starts_with(UNDERSCORE)) else {
            return;
        };
        let first = &methods[at].name;
        methods[at..]
            .iter()
            .filter(|method| Name::private(&method.name, publics))
            .for_each(|method| {
                let detail = format!("{} comes after public {first}", method.name);
                sink.report(&self.rule, path, method.span.clone(), Some(&detail), None);
            });
    }

    fn judge(&self, path: &Path, class: &Class, ancestors: &[Ancestor], sink: &mut Sink) {
        let publics: BTreeSet<&str> = std::iter::once(class)
            .chain(ancestors.iter().map(|ancestor| &*ancestor.class))
            .flat_map(Name::publics)
            .collect();
        let methods = self.judged(class, ancestors, &publics);
        self.lineage(path, class, &methods, &publics, sink);
        self.order(path, &methods, &publics, sink);
    }

    /// A rule judging the methods of every class in its scope.
    #[must_use]
    pub fn new(rule: &Rule, spec: &MembersSpec, scope: ScopeMatcher) -> Self {
        Self {
            rule: rule.clone(),
            scope,
            spec: spec.clone(),
        }
    }
}

impl MembersEngine {
    /// The engine over its rules, its bases resolved by the run's resolver.
    #[must_use]
    pub(crate) fn new(rules: Vec<MembersRule>, ancestry: Arc<Ancestry>) -> Self {
        Self { rules, ancestry }
    }
}

impl FileEngine for MembersEngine {
    fn admits(&self, path: &Path) -> bool {
        self.rules.iter().any(|rule| rule.scope.admits(path))
    }

    fn check(&self, source: &Source<'_>, sink: &mut Sink) {
        let path = source.path;
        let outline = Arc::new(Outline::new(path, &source.root.root()));
        let rules: Vec<&MembersRule> = self.rules.iter().filter(|rule| rule.scope.admits(path)).collect();
        for class in &outline.every {
            let line = match self.ancestry.of(&outline, class) {
                Ok(line) => line,
                Err(missing) => {
                    sink.census.skip(&missing.name, missing.why);
                    continue;
                }
            };
            sink.census.judged += 1;
            sink.census.builtin += usize::from(line.builtin);
            for rule in &rules {
                rule.judge(path, class, &line.ancestors, sink);
            }
        }
    }
}
