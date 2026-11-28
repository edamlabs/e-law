use std::path::Path;

use law_model::{Classes, Constants, Functions, Rule, ShapeSpec};

use crate::engine::{FileEngine, Sink};
use crate::fault::Fault;
use crate::scope::ScopeMatcher;
use crate::source::{Source, Syntax};

const CLASS: &str = "class_definition";
const FUNCTION: &str = "function_definition";
const DECORATED: &str = "decorated_definition";
const STATEMENT: &str = "expression_statement";
const ASSIGNMENT: &str = "assignment";
const KEYWORD: &str = "keyword_argument";
const COMMENT: &str = "comment";
const STRING: &str = "string";
const CONTENT: &str = "string_content";
const ASYNC: &str = "async";
const NAME: &str = "name";
const BASES: &str = "superclasses";
const DEFINITION: &str = "definition";
const ANNOTATION: &str = "type";
const VALUE: &str = "right";
const QUOTES: [char; 2] = ['"', '\''];

/// A `shape` rule with its scope.
pub struct ShapeRule {
    rule: Rule,
    scope: ScopeMatcher,
    spec: ShapeSpec,
}

/// The `shape` engine: what the top level of each module holds.
pub struct ShapeEngine {
    rules: Vec<ShapeRule>,
}

struct Module<'a> {
    classes: Vec<Syntax<'a>>,
    functions: Vec<Syntax<'a>>,
    constants: Vec<(Syntax<'a>, Syntax<'a>)>,
}

impl<'a> Module<'a> {
    fn definition(node: Syntax<'a>) -> Syntax<'a> {
        match &*node.kind() {
            DECORATED => node.field(DEFINITION).unwrap_or(node),
            _ => node,
        }
    }

    fn assignment(node: &Syntax<'a>) -> Option<Syntax<'a>> {
        (node.kind() == STATEMENT)
            .then(|| node.named_children().next())
            .flatten()
            .filter(|child| child.kind() == ASSIGNMENT)
    }

    fn new(source: &'a Source<'_>) -> Self {
        let top: Vec<Syntax<'a>> = source.root.root().named_children().map(Self::definition).collect();
        let of = |kind: &str| top.iter().filter(|node| node.kind() == kind).cloned().collect();
        Self {
            classes: of(CLASS),
            functions: of(FUNCTION),
            constants: top
                .iter()
                .filter_map(|node| Self::assignment(node).map(|assignment| (node.clone(), assignment)))
                .collect(),
        }
    }
}

impl ShapeRule {
    fn name(node: &Syntax<'_>) -> String {
        node.field(NAME)
            .map(|name| name.text().into_owned())
            .unwrap_or_default()
    }

    fn base(node: &Syntax<'_>) -> String {
        let text = node.text();
        let bare = text.split('[').next().unwrap_or_default();
        bare.rsplit('.').next().unwrap_or_default().trim().to_owned()
    }

    fn literal(node: &Syntax<'_>) -> bool {
        node.kind() == STRING && {
            let text = node.text();
            let prefix = text.split(QUOTES).next().unwrap_or_default().to_lowercase();
            !prefix.contains('b') && (!prefix.contains('f') || node.children().any(|child| child.kind() == CONTENT))
        }
    }

    fn classes(&self, spec: &Classes, source: &Source<'_>, module: &Module<'_>, sink: &mut Sink) {
        let path = source.path;
        let stem = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or_default();
        let dir = path
            .parent()
            .and_then(Path::file_name)
            .and_then(|dir| dir.to_str())
            .unwrap_or_default();
        let count = module.classes.len();
        spec.count
            .filter(|&allowed| usize::try_from(allowed).ok() != Some(count))
            .into_iter()
            .for_each(|allowed| {
                let detail = format!("{count} classes, {allowed} allowed");
                sink.report(&self.rule, path, Sink::origin(), Some(&detail), None);
            });
        match spec.name.of(dir, stem) {
            Ok(glob) => module
                .classes
                .iter()
                .map(|node| (node, Self::name(node)))
                .filter(|(_, name)| !glob.matches(name))
                .for_each(|(node, name)| {
                    let detail = format!("class {name} is not named {glob}");
                    sink.report(&self.rule, path, Source::span(node), Some(&detail), None);
                }),
            Err(invalid) => sink.faults.push(Fault::BadTemplate {
                rule: self.rule.id.clone(),
                path: path.to_owned(),
                message: invalid.to_string(),
            }),
        }
        spec.bases.iter().for_each(|bases| {
            module
                .classes
                .iter()
                .filter(|node| {
                    !node.field(BASES).is_some_and(|list| {
                        list.named_children()
                            .filter(|base| base.kind() != KEYWORD && base.kind() != COMMENT)
                            .any(|base| bases.matches(Self::base(&base)))
                    })
                })
                .for_each(|node| {
                    let detail = format!("class {} has no base named {bases}", Self::name(node));
                    sink.report(&self.rule, path, Source::span(node), Some(&detail), None);
                });
        });
    }

    fn functions(&self, spec: &Functions, source: &Source<'_>, module: &Module<'_>, sink: &mut Sink) {
        let path = source.path;
        spec.max
            .and_then(|max| usize::try_from(max).ok().map(|skip| (max, skip)))
            .into_iter()
            .flat_map(|(max, skip)| module.functions.iter().skip(skip).map(move |node| (max, node)))
            .for_each(|(max, node)| {
                let detail = format!("function {} beyond the {max} allowed", Self::name(node));
                sink.report(&self.rule, path, Source::span(node), Some(&detail), None);
            });
        module
            .functions
            .iter()
            .filter(|node| spec.asynchronous && node.children().next().is_none_or(|first| first.kind() != ASYNC))
            .for_each(|node| {
                let detail = format!("function {} is not async", Self::name(node));
                sink.report(&self.rule, path, Source::span(node), Some(&detail), None);
            });
    }

    fn constants(&self, spec: &Constants, source: &Source<'_>, module: &Module<'_>, sink: &mut Sink) {
        let path = source.path;
        spec.max
            .and_then(|max| usize::try_from(max).ok().map(|skip| (max, skip)))
            .into_iter()
            .flat_map(|(max, skip)| module.constants.iter().skip(skip).map(move |(node, _)| (max, node)))
            .for_each(|(max, node)| {
                let detail = format!("constant beyond the {max} allowed");
                sink.report(&self.rule, path, Source::span(node), Some(&detail), None);
            });
        spec.annotation.iter().for_each(|expected| {
            module
                .constants
                .iter()
                .map(|(node, assignment)| {
                    let annotation = assignment
                        .field(ANNOTATION)
                        .map(|annotation| annotation.text().into_owned())
                        .unwrap_or_default();
                    (node, annotation)
                })
                .filter(|(_, annotation)| !expected.matches(annotation))
                .for_each(|(node, annotation)| {
                    let shown = if annotation.is_empty() { "none" } else { &annotation };
                    let detail = format!("annotation {shown} is not {expected}");
                    sink.report(&self.rule, path, Source::span(node), Some(&detail), None);
                });
        });
        module
            .constants
            .iter()
            .filter(|(_, assignment)| {
                !spec.literals
                    && assignment
                        .field(VALUE)
                        .is_some_and(|value| value.dfs().any(|node| Self::literal(&node)))
            })
            .for_each(|(node, _)| {
                sink.report(
                    &self.rule,
                    path,
                    Source::span(node),
                    Some("a string literal in a constant"),
                    None,
                );
            });
    }

    fn check(&self, source: &Source<'_>, module: &Module<'_>, sink: &mut Sink) {
        self.spec
            .classes
            .iter()
            .for_each(|spec| self.classes(spec, source, module, sink));
        self.spec
            .functions
            .iter()
            .for_each(|spec| self.functions(spec, source, module, sink));
        self.spec
            .constants
            .iter()
            .for_each(|spec| self.constants(spec, source, module, sink));
    }

    /// A rule whose shape is checked whole.
    #[must_use]
    pub fn new(rule: &Rule, spec: &ShapeSpec, scope: ScopeMatcher) -> Self {
        Self {
            rule: rule.clone(),
            scope,
            spec: spec.clone(),
        }
    }
}

impl ShapeEngine {
    /// The engine over its rules.
    #[must_use]
    pub fn new(rules: Vec<ShapeRule>) -> Self {
        Self { rules }
    }
}

impl FileEngine for ShapeEngine {
    fn admits(&self, path: &Path) -> bool {
        self.rules.iter().any(|rule| rule.scope.admits(path))
    }

    fn check(&self, source: &Source<'_>, sink: &mut Sink) {
        let module = Module::new(source);
        self.rules
            .iter()
            .filter(|rule| rule.scope.admits(source.path))
            .for_each(|rule| rule.check(source, &module, sink));
    }
}
