use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ast_grep_language::{LanguageExt, SupportLang};
use law_model::{Of, ParitySpec, Rule, Span};

use crate::engine::{FileEngine, Sink};
use crate::fault::Fault;
use crate::listing::Listing;
use crate::outline::{Class, Outline};
use crate::scope::ScopeMatcher;
use crate::source::Source;
use crate::walk::PYTHON;

const DUNDER: &str = "__";
const INIT: &str = "__init__";
const BESIDE: &str = "module beside it";

/// A `parity` rule with its scope: the files that declare names.
pub struct ParityRule {
    rule: Rule,
    scope: ScopeMatcher,
    spec: ParitySpec,
}

/// The `parity` engine: the names each declaring file's classes hold against the modules beside the file, or the files
/// of a glob, listed once per judged file.
pub struct ParityEngine {
    rules: Vec<ParityRule>,
    root: PathBuf,
}

impl ParityRule {
    fn here(path: &Path) -> &Path {
        path.parent().unwrap_or_else(|| Path::new(""))
    }

    fn dir(path: &Path) -> &str {
        Self::here(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
    }

    fn stem(path: &Path) -> &str {
        path.file_stem().and_then(|stem| stem.to_str()).unwrap_or_default()
    }

    fn named(path: &Path, files: &[PathBuf]) -> BTreeSet<String> {
        let dir = Self::here(path);
        let own = path.file_stem();
        files
            .iter()
            .filter(|file| file.extension().is_some_and(|extension| extension == PYTHON))
            .filter_map(|file| {
                let mut parts = file.strip_prefix(dir).ok()?.components();
                let first = parts.next()?.as_os_str();
                let entry = if parts.next().is_some() {
                    first
                } else {
                    Path::new(first).file_stem()?
                };
                entry.to_str().map(str::to_owned)
            })
            .filter(|entry| entry != INIT && own.is_none_or(|own| entry.as_str() != own))
            .collect()
    }

    fn entries(&self, path: &Path, listing: &Listing<'_>, faults: &mut Vec<Fault>) -> BTreeSet<String> {
        let spec = &self.spec.entries;
        let mut found = match &spec.glob {
            Some(glob) => listing
                .files(&Listing::prefix(glob.as_str()), faults)
                .iter()
                .filter(|file| glob.matches(file))
                .filter_map(|file| file.file_stem().and_then(|stem| stem.to_str()).map(str::to_owned))
                .collect(),
            None => Self::named(path, &listing.files(Self::here(path), faults)),
        };
        found.retain(|entry| !spec.exclude.contains(entry));
        found
    }

    fn names<'a>(&self, class: &'a Class) -> Vec<(&'a str, &'a Span)> {
        let valued = self.spec.names.of == Of::Members;
        class
            .bindings
            .iter()
            .filter(|binding| !(binding.name.starts_with(DUNDER) && binding.name.ends_with(DUNDER)))
            .filter(|binding| binding.valued || !valued)
            .map(|binding| (binding.name.as_str(), &binding.span))
            .collect()
    }

    fn key(&self, name: &str) -> String {
        match self.spec.names.of {
            Of::Attributes => name.to_owned(),
            Of::Members => name.to_lowercase(),
        }
    }

    fn judge(&self, path: &Path, class: &Class, entries: &BTreeSet<String>, sink: &mut Sink) {
        let relation = self.spec.relation;
        let names = self.names(class);
        let held: BTreeSet<String> = names.iter().map(|(name, _)| self.key(name)).collect();
        let target = self
            .spec
            .entries
            .glob
            .as_ref()
            .map_or_else(|| BESIDE.to_owned(), |glob| format!("file of {}", glob.as_str()));
        if relation.names() {
            names
                .iter()
                .filter(|(name, _)| !entries.contains(&self.key(name)))
                .for_each(|(name, span)| {
                    let detail = format!("{name} names no {target}");
                    sink.report(&self.rule, path, (*span).clone(), Some(&detail), None);
                });
        }
        if relation.entries() {
            entries.iter().filter(|entry| !held.contains(*entry)).for_each(|entry| {
                let detail = format!("{} has no {} for {entry}", class.name, self.spec.names.of.singular());
                sink.report(&self.rule, path, class.span.clone(), Some(&detail), None);
            });
        }
    }

    fn file(&self, outline: &Outline, classes: &[&Arc<Class>], listing: &Listing<'_>, sink: &mut Sink) {
        let path = outline.path.as_path();
        let entries = self.entries(path, listing, &mut sink.faults);
        let name = self.spec.names.class.of(Self::dir(path), Self::stem(path));
        let mut judged = classes.iter().filter(|class| class.name == name).peekable();
        if judged.peek().is_none() {
            let detail = format!("no class named {name}");
            sink.report(&self.rule, path, Sink::origin(), Some(&detail), None);
        }
        judged.for_each(|class| self.judge(path, class, &entries, sink));
    }

    /// A rule judging the names of the classes of every file in its scope.
    #[must_use]
    pub fn new(rule: &Rule, spec: &ParitySpec, scope: ScopeMatcher) -> Self {
        Self {
            rule: rule.clone(),
            scope,
            spec: spec.clone(),
        }
    }
}

impl ParityEngine {
    fn declared(outline: &Outline) -> Vec<&Arc<Class>> {
        outline
            .every
            .iter()
            .filter(|class| {
                outline
                    .classes
                    .get(&class.name)
                    .is_some_and(|own| Arc::ptr_eq(own, class))
            })
            .collect()
    }

    fn judge(&self, outline: &Outline, listing: &Listing<'_>, sink: &mut Sink) {
        let classes = Self::declared(outline);
        self.rules
            .iter()
            .filter(|rule| rule.scope.admits(&outline.path))
            .for_each(|rule| rule.file(outline, &classes, listing, sink));
    }

    /// The engine over its rules, its entries listed from the project root handed in.
    #[must_use]
    pub fn new(rules: Vec<ParityRule>, root: PathBuf) -> Self {
        Self { rules, root }
    }

    /// The findings of its rules over the files of one case, each a path and its text, listed among themselves only.
    pub fn case(&self, files: &[(&Path, &str)], sink: &mut Sink) {
        let paths: Vec<PathBuf> = files.iter().map(|(path, _)| (*path).to_owned()).collect();
        let listing = Listing::Case(&paths);
        files
            .iter()
            .filter(|(path, _)| self.admits(path))
            .for_each(|(path, text)| {
                let tree = SupportLang::Python.ast_grep(text);
                self.judge(&Outline::new(path, &tree.root()), &listing, sink);
            });
    }
}

impl FileEngine for ParityEngine {
    fn admits(&self, path: &Path) -> bool {
        self.rules.iter().any(|rule| rule.scope.admits(path))
    }

    fn check(&self, source: &Source<'_>, sink: &mut Sink) {
        self.judge(
            &Outline::new(source.path, &source.root.root()),
            &Listing::Disk(&self.root),
            sink,
        );
    }
}
