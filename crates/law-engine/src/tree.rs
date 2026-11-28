use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use law_model::{Rule, TreeSpec};

use crate::engine::Sink;
use crate::fault::Fault;
use crate::scope::ScopeMatcher;

/// The directory name never read as an entry, nor checked as a directory.
pub(crate) const SKIPPED: &str = "__pycache__";

/// A `tree` rule with its scope; `dirs` is where it looks, the scope only fences.
pub struct TreeRule {
    rule: Rule,
    scope: ScopeMatcher,
    spec: TreeSpec,
}

/// The `tree` engine: what each directory holds.
pub struct TreeEngine {
    rules: Vec<TreeRule>,
}

impl TreeRule {
    fn looks(&self, dir: &Path) -> bool {
        self.spec.dirs.iter().any(|glob| glob.matches(dir)) && !self.scope.fences(dir)
    }

    fn check(&self, dir: &Path, entries: &BTreeSet<String>, sink: &mut Sink) {
        let mut report = |detail: &str| sink.report(&self.rule, dir, Sink::origin(), Some(detail), None);
        self.spec.allow.iter().for_each(|allow| {
            entries
                .iter()
                .filter(|entry| !allow.iter().any(|glob| glob.matches(entry.as_str())))
                .for_each(|entry| report(&format!("{entry} is not allowed")));
        });
        self.spec.deny.iter().for_each(|deny| {
            entries
                .iter()
                .filter(|entry| deny.matches(entry.as_str()))
                .for_each(|entry| report(&format!("{entry} is banned")));
        });
        self.spec
            .require
            .iter()
            .filter(|glob| !entries.iter().any(|entry| glob.matches(entry.as_str())))
            .for_each(|glob| report(&format!("{glob} is missing")));
        self.spec.name.iter().for_each(|name| {
            entries
                .iter()
                .filter(|entry| {
                    self.spec
                        .only
                        .as_ref()
                        .is_none_or(|only| only.iter().any(|glob| glob.matches(entry.as_str())))
                })
                .filter(|entry| !name.matches(entry.trim_end_matches('/')))
                .for_each(|entry| report(&format!("{entry} does not match {}", name.as_str())));
        });
    }

    /// A rule over directories.
    #[must_use]
    pub fn new(rule: &Rule, spec: &TreeSpec, scope: ScopeMatcher) -> Self {
        Self {
            rule: rule.clone(),
            scope,
            spec: spec.clone(),
        }
    }
}

impl TreeEngine {
    fn entries(root: &Path, dir: &Path) -> Result<BTreeSet<String>, Fault> {
        let full = root.join(dir);
        fs::read_dir(&full)
            .and_then(|read| {
                read.map(|entry| {
                    entry.and_then(|entry| {
                        let name = entry.file_name().to_string_lossy().into_owned();
                        let slash = if entry.file_type()?.is_dir() { "/" } else { "" };
                        Ok((name, slash))
                    })
                })
                .filter(|entry| !entry.as_ref().is_ok_and(|(name, _)| name == SKIPPED))
                .map(|entry| entry.map(|(name, slash)| format!("{name}{slash}")))
                .collect()
            })
            .map_err(|error| Fault::io(&full, &error))
    }

    /// The engine over its rules.
    #[must_use]
    pub fn new(rules: Vec<TreeRule>) -> Self {
        Self { rules }
    }

    /// The findings of every rule that looks at a directory, its path from `root`.
    pub fn check(&self, root: &Path, dir: &Path, sink: &mut Sink) {
        let looking: Vec<&TreeRule> = self.rules.iter().filter(|rule| rule.looks(dir)).collect();
        if looking.is_empty() {
            return;
        }
        match Self::entries(root, dir) {
            Ok(entries) => looking.iter().for_each(|rule| rule.check(dir, &entries, sink)),
            Err(fault) => sink.faults.push(fault),
        }
    }
}
