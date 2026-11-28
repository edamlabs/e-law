use std::collections::HashSet;
use std::path::Path;

use law_model::{GraphAspect, GraphSpec, Rule};

use crate::digraph::Graph;
use crate::engine::{ProjectEngine, Sink};
use crate::scope::ScopeMatcher;

const ARROW: &str = " \u{2192} ";
const ORPHAN: &str = "nothing imports it";

/// A `graph` rule with its scope: the modules that may carry its findings, never those in the graph.
pub struct GraphRule {
    rule: Rule,
    scope: ScopeMatcher,
    spec: GraphSpec,
}

/// The `graph` engine: every module under the project's source roots and the imports between them, judged whole.
pub struct GraphEngine {
    rules: Vec<GraphRule>,
}

impl GraphRule {
    fn cycles(&self, graph: &Graph, asked: Option<&HashSet<&Path>>, sink: &mut Sink) {
        let nodes = graph.nodes();
        graph
            .components(self.spec.typing)
            .iter()
            .filter_map(|component| {
                let start = component.iter().copied().find(|&at| {
                    let path = &nodes[at].path;
                    self.scope.admits(path) && asked.is_none_or(|asked| asked.contains(path.as_path()))
                })?;
                graph.cycle(start, component, self.spec.typing)
            })
            .for_each(|cycle| {
                let names: Vec<&str> = cycle
                    .modules
                    .iter()
                    .chain(cycle.modules.first())
                    .map(|&at| nodes[at].name.as_str())
                    .collect();
                let edge = &graph.edges()[cycle.edge];
                let path = &nodes[edge.from].path;
                sink.report(&self.rule, path, edge.span.clone(), Some(&names.join(ARROW)), None);
            });
    }

    fn orphans(&self, graph: &Graph, sink: &mut Sink) {
        let nodes = graph.nodes();
        graph
            .orphans()
            .into_iter()
            .map(|at| &nodes[at].path)
            .filter(|path| self.scope.admits(path) && !self.spec.entries.iter().any(|entry| entry.matches(path)))
            .for_each(|path| sink.report(&self.rule, path, Sink::origin(), Some(ORPHAN), None));
    }

    /// A rule over the import graph.
    #[must_use]
    pub fn new(rule: &Rule, spec: &GraphSpec, scope: ScopeMatcher) -> Self {
        Self {
            rule: rule.clone(),
            scope,
            spec: spec.clone(),
        }
    }
}

impl GraphEngine {
    /// The engine over its rules.
    #[must_use]
    pub fn new(rules: Vec<GraphRule>) -> Self {
        Self { rules }
    }

    /// Whether it has a rule to judge: with none, no graph is built.
    #[must_use]
    pub fn judges(&self) -> bool {
        !self.rules.is_empty()
    }
}

impl ProjectEngine for GraphEngine {
    fn check(&self, graph: &Graph, asked: Option<&HashSet<&Path>>, sink: &mut Sink) {
        self.rules.iter().for_each(|rule| match rule.spec.aspect {
            GraphAspect::Cycle => rule.cycles(graph, asked, sink),
            GraphAspect::Orphan => rule.orphans(graph, sink),
        });
    }
}
