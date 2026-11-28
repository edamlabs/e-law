use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use law_model::Span;
use petgraph::algo::tarjan_scc;
use petgraph::graph::{DiGraph, NodeIndex};
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};

use crate::ancestry::Ancestry;
use crate::outline::{Import, Outline};

const DOT: char = '.';
const PACKAGE: &str = "__init__";

/// A module of the graph: its path from the project root, its dotted name, and whether it is a package's `__init__`.
pub(crate) struct Node {
    pub(crate) path: PathBuf,
    pub(crate) name: String,
    pub(crate) package: bool,
}

/// An import of one module by another, where its first statement is written — one outside `if TYPE_CHECKING:` first —,
/// and whether every statement of it sits under `if TYPE_CHECKING:`.
pub(crate) struct Edge {
    pub(crate) from: usize,
    pub(crate) to: usize,
    pub(crate) span: Span,
    pub(crate) typing: bool,
    written: Option<String>,
}

/// A cycle through a module, its shortest: the modules in order, the first the one it runs through, and the edge that
/// leaves it.
pub(crate) struct Cycle {
    pub(crate) modules: Vec<usize>,
    pub(crate) edge: usize,
}

/// How big a graph is: its modules, its edges, and how many of those only `if TYPE_CHECKING:` draws.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Extent {
    /// The modules.
    pub modules: usize,
    /// The edges, one per importing and imported module.
    pub edges: usize,
    /// The edges every statement of which sits under `if TYPE_CHECKING:`.
    pub typing: usize,
}

/// The import graph of a project: every module under its source roots, and an edge from each to every project module
/// one of its import statements names, resolved as the resolver of bases resolves modules; edges sorted by their two
/// ends, the first edge of each module at its offset.
pub struct Graph {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    offsets: Vec<usize>,
}

impl Node {
    fn of(ancestry: &Ancestry, outline: &Arc<Outline>) -> Option<(Self, Arc<Outline>)> {
        let name = ancestry.named(&outline.path)?;
        let package = outline.path.file_stem().is_some_and(|stem| stem == PACKAGE);
        let node = Self {
            path: outline.path.clone(),
            name,
            package,
        };
        Some((node, Arc::clone(outline)))
    }

    /// The package its relative imports resolve against: itself when a package's `__init__`, else its parent.
    pub(crate) fn within(&self) -> &str {
        if self.package {
            &self.name
        } else {
            self.name.rsplit_once(DOT).map_or("", |(parent, _)| parent)
        }
    }
}

impl Graph {
    fn targets(package: &str, import: &Import) -> Vec<String> {
        let Some(module) = Ancestry::anchored(package, &import.module) else {
            return Vec::new();
        };
        let named = import.names.iter().map(|name| {
            if module.is_empty() {
                name.clone()
            } else {
                format!("{module}{DOT}{name}")
            }
        });
        std::iter::once(module.clone())
            .filter(|module| !module.is_empty())
            .chain(named)
            .collect()
    }

    fn spell(
        ancestry: &Ancestry,
        index: &HashMap<&Path, usize>,
        read: &[(Node, Arc<Outline>)],
        edges: &[Edge],
    ) -> Vec<String> {
        let mut written: Vec<Option<&str>> = vec![None; read.len()];
        for edge in edges {
            written[edge.to] = written[edge.to].or(edge.written.as_deref());
        }
        read.iter()
            .zip(written)
            .map(|((node, _), written)| {
                let reaches = |name: &String| {
                    ancestry
                        .project(name, |path| index.contains_key(path))
                        .is_some_and(|found| found == node.path)
                };
                written
                    .map(str::to_owned)
                    .into_iter()
                    .chain([node.name.clone(), Ancestry::spelled(&node.path)])
                    .find(reaches)
                    .unwrap_or_else(|| node.name.clone())
            })
            .collect()
    }

    fn drawn(
        ancestry: &Ancestry,
        index: &HashMap<&Path, usize>,
        from: usize,
        node: &Node,
        outline: &Outline,
    ) -> Vec<Edge> {
        outline
            .statements
            .iter()
            .flat_map(|import| {
                Self::targets(node.within(), import)
                    .into_iter()
                    .filter_map(|module| {
                        let path = ancestry.project(&module, |path| index.contains_key(path))?;
                        Some((module, index.get(path.as_path()).copied()?))
                    })
                    .map(|(module, to)| Edge {
                        from,
                        to,
                        span: import.span.clone(),
                        typing: import.typing,
                        written: (!import.module.starts_with(DOT)).then_some(module),
                    })
            })
            .collect()
    }

    /// The graph of the modules outlined, each named and its imports resolved by the resolver handed in; a module
    /// outside every source root is no node.
    pub(crate) fn new(ancestry: &Ancestry, outlines: &[Arc<Outline>]) -> Self {
        let mut read: Vec<(Node, Arc<Outline>)> = outlines
            .iter()
            .filter_map(|outline| Node::of(ancestry, outline))
            .collect();
        read.sort_by(|(a, _), (b, _)| a.path.cmp(&b.path));
        read.dedup_by(|(a, _), (b, _)| a.path == b.path);
        let index: HashMap<&Path, usize> = read
            .iter()
            .enumerate()
            .map(|(at, (node, _))| (node.path.as_path(), at))
            .collect();
        let numbered: Vec<(usize, &(Node, Arc<Outline>))> = read.iter().enumerate().collect();
        let mut edges: Vec<Edge> = numbered
            .par_iter()
            .flat_map_iter(|(from, (node, outline))| Self::drawn(ancestry, &index, *from, node, outline))
            .collect();
        edges.sort_by(|a, b| {
            (a.from, a.to, a.typing, a.span.start.line).cmp(&(b.from, b.to, b.typing, b.span.start.line))
        });
        edges.dedup_by(|next, kept| (next.from, next.to) == (kept.from, kept.to));
        let names = Self::spell(ancestry, &index, &read, &edges);
        let nodes: Vec<Node> = read
            .into_iter()
            .zip(names)
            .map(|((node, _), name)| Node { name, ..node })
            .collect();
        let mut offsets = vec![0; nodes.len() + 1];
        for edge in &edges {
            offsets[edge.from + 1] += 1;
        }
        (0..nodes.len()).for_each(|at| offsets[at + 1] += offsets[at]);
        Self { nodes, edges, offsets }
    }

    /// Its modules.
    pub(crate) fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// Its edges.
    pub(crate) fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// The module at a path from the project root.
    pub(crate) fn at(&self, path: &Path) -> Option<usize> {
        self.nodes.binary_search_by(|node| node.path.as_path().cmp(path)).ok()
    }

    /// The modules, other than itself, an import statement of which names the module: each once, by path.
    pub(crate) fn importers(&self, module: usize) -> Vec<usize> {
        let mut found: Vec<usize> = self
            .edges
            .iter()
            .filter(|edge| edge.to == module && edge.from != module)
            .map(|edge| edge.from)
            .collect();
        found.dedup();
        found
    }

    /// How big it is.
    #[must_use]
    pub fn extent(&self) -> Extent {
        Extent {
            modules: self.nodes.len(),
            edges: self.edges.len(),
            typing: self.edges.iter().filter(|edge| edge.typing).count(),
        }
    }

    fn leaving(&self, from: usize, typing: bool) -> impl Iterator<Item = (usize, &Edge)> {
        (self.offsets[from]..self.offsets[from + 1])
            .map(|at| (at, &self.edges[at]))
            .filter(move |(_, edge)| typing || !edge.typing)
    }

    /// Every set of modules that import one another round — a strongly connected component of more than one module,
    /// or a module importing itself —, each sorted by name then path, in the order of their first; an edge drawn only
    /// under `if TYPE_CHECKING:` counted when `typing` says so.
    pub(crate) fn components(&self, typing: bool) -> Vec<Vec<usize>> {
        let mut graph = DiGraph::<(), ()>::with_capacity(self.nodes.len(), self.edges.len());
        self.nodes.iter().for_each(|_| {
            graph.add_node(());
        });
        graph.extend_with_edges(
            self.edges
                .iter()
                .filter(|edge| typing || !edge.typing)
                .filter_map(|edge| Some((u32::try_from(edge.from).ok()?, u32::try_from(edge.to).ok()?))),
        );
        let order = |at: &usize| (&self.nodes[*at].name, &self.nodes[*at].path);
        let mut components: Vec<Vec<usize>> = tarjan_scc(&graph)
            .into_iter()
            .map(|component| component.into_iter().map(NodeIndex::index).collect::<Vec<usize>>())
            .filter(|component| {
                component.len() > 1
                    || component
                        .first()
                        .is_some_and(|&only| self.leaving(only, typing).any(|(_, edge)| edge.to == only))
            })
            .map(|mut component| {
                component.sort_by_key(order);
                component
            })
            .collect();
        components.sort_by(|a, b| a.first().map(order).cmp(&b.first().map(order)));
        components
    }

    /// The shortest cycle through a module that stays within its component, by a breadth-first search from it.
    pub(crate) fn cycle(&self, start: usize, component: &[usize], typing: bool) -> Option<Cycle> {
        let members: HashSet<usize> = component.iter().copied().collect();
        let mut parents: HashMap<usize, (usize, usize)> = HashMap::new();
        let mut frontier = VecDeque::from([start]);
        let mut closed = None;
        while let Some(at) = frontier.pop_front() {
            if let Some((edge, _)) = self.leaving(at, typing).find(|(_, edge)| edge.to == start) {
                closed = Some((at, edge));
                break;
            }
            self.leaving(at, typing)
                .filter(|(_, edge)| members.contains(&edge.to) && edge.to != start)
                .for_each(|(edge, next)| {
                    if let Entry::Vacant(seen) = parents.entry(next.to) {
                        seen.insert((at, edge));
                        frontier.push_back(next.to);
                    }
                });
        }
        let (last, closing) = closed?;
        let mut modules = vec![last];
        let mut edge = closing;
        let mut at = last;
        while let Some(&(parent, through)) = parents.get(&at) {
            modules.push(parent);
            edge = through;
            at = parent;
        }
        modules.reverse();
        Some(Cycle { modules, edge })
    }

    /// The modules no other module imports: no edge reaches them but their own; a package's `__init__` reached too
    /// when another module imports a module inside the package.
    pub(crate) fn orphans(&self) -> Vec<usize> {
        let mut imported = vec![false; self.nodes.len()];
        let mut packages: HashSet<&str> = HashSet::new();
        self.edges.iter().filter(|edge| edge.from != edge.to).for_each(|edge| {
            imported[edge.to] = true;
            let name = self.nodes[edge.to].name.as_str();
            let source = self.nodes[edge.from].name.as_str();
            name.match_indices(DOT)
                .map(|(at, _)| &name[..at])
                .filter(|package| *package != source)
                .for_each(|package| {
                    packages.insert(package);
                });
        });
        self.nodes
            .iter()
            .enumerate()
            .filter(|(at, node)| !(imported[*at] || node.package && packages.contains(node.name.as_str())))
            .map(|(at, _)| at)
            .collect()
    }
}
