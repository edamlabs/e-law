use std::collections::{BTreeMap, HashSet};
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::{self, Scope, ScopedJoinHandle};
use std::time::{Duration, Instant};

use ast_grep_language::{LanguageExt, SupportLang};
use jiff::Timestamp;
use jiff::civil::Date;
use law_config::{Loaded, Selection};
use law_model::{Catalogue, Former, Report, RuleId, Stamp, Tags, Version};
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::ancestry::Ancestry;
use crate::bank::Bank;
use crate::census::Census;
use crate::digraph::{Extent, Graph};
use crate::engine::{Author, BatchEngine, FileEngine, ProjectEngine, Sink};
use crate::environment::Environment;
use crate::exception::ExceptionLedger;
use crate::fault::Fault;
use crate::graph::GraphEngine;
use crate::outline::Outline;
use crate::plan::Plan;
use crate::pragma::PragmaEngine;
use crate::reserved::{Reserved, ReservedRule};
use crate::scope::ScopeMatcher;
use crate::source::Source;
use crate::timing::Timing;
use crate::tree::TreeEngine;
use crate::walk::Walk;

const HERE: &str = ".";
const SHA256: &str = "sha256:";
const PANICKED: &str = "the engine panicked";
const WALK: &str = "walk";
const PARSE: &str = "parse";
const TREE: &str = "tree";
const GRAPH: &str = "graph";

/// A run planned once: the admitted rules compiled into their engines, the exceptions split by date.
pub struct Run {
    root: PathBuf,
    roots: Vec<PathBuf>,
    selection: Selection,
    stamps: Vec<Stamp>,
    files: Vec<Box<dyn FileEngine>>,
    tree: TreeEngine,
    graph: GraphEngine,
    batch: Vec<Box<dyn BatchEngine>>,
    external: BTreeMap<RuleId, ScopeMatcher>,
    ledger: ExceptionLedger,
    known: BTreeMap<RuleId, Tags>,
    former: BTreeMap<Former, RuleId>,
    messages: BTreeMap<RuleId, String>,
    admitted: Vec<(RuleId, Tags)>,
    pragma: ReservedRule,
    expired: ReservedRule,
    unused: ReservedRule,
    ancestry: Arc<Ancestry>,
    profiled: bool,
    metered: bool,
}

/// What a run produced: its report, the faults of the paths it could not check, the classes it judged, how big the
/// import graph was, when a rule judged it, and how long each phase took.
#[derive(Debug)]
pub struct Outcome {
    /// The report.
    pub report: Report,
    /// The faults, in walk order.
    pub faults: Vec<Fault>,
    /// The classes the `members` engine judged and skipped.
    pub census: Census,
    /// The import graph's extent, when a `graph` rule ran.
    pub graph: Option<Extent>,
    /// The phases, in the order they ended.
    pub timings: Vec<Timing>,
}

impl Run {
    fn canonical(value: Value) -> Value {
        match value {
            Value::Object(map) => {
                let mut entries: Vec<(String, Value)> = map
                    .into_iter()
                    .map(|(key, value)| (key, Self::canonical(value)))
                    .collect();
                entries.sort_by(|(a, _), (b, _)| a.cmp(b));
                Value::Object(entries.into_iter().collect())
            }
            Value::Array(items) => Value::Array(items.into_iter().map(Self::canonical).collect()),
            other => other,
        }
    }

    fn stamp(catalogue: &Catalogue) -> Result<Stamp, Fault> {
        let text = serde_json::to_value(catalogue)
            .and_then(|value| serde_json::to_vec(&Self::canonical(value)))
            .map_err(|error| Fault::Digest {
                catalogue: catalogue.name.clone(),
                message: error.to_string(),
            })?;
        let digest = Sha256::digest(&text).iter().fold(SHA256.to_owned(), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        });
        Ok(Stamp {
            name: catalogue.name.clone(),
            version: catalogue.version.clone(),
            digest,
        })
    }

    fn read(root: &Path, path: &Path) -> Result<String, Fault> {
        fs::read(root.join(path))
            .map_err(|error| Fault::io(path, &error))
            .and_then(|bytes| {
                String::from_utf8(bytes).map_err(|error| Fault::Io {
                    path: path.to_owned(),
                    message: error.to_string(),
                })
            })
    }

    fn file(&self, root: &Path, path: &Path) -> (Sink, Vec<Arc<Outline>>) {
        let mut sink = Sink::default();
        let engines: Vec<&dyn FileEngine> = self
            .files
            .iter()
            .map(AsRef::as_ref)
            .filter(|engine| engine.admits(path))
            .collect();
        let outlined = self.graph.judges() && self.ancestry.named(path).is_some();
        if engines.is_empty() && !outlined {
            return (sink, Vec::new());
        }
        match Self::read(root, path) {
            Ok(text) => {
                let tree = SupportLang::Python.ast_grep(&text);
                let source = Source {
                    path,
                    text: &text,
                    root: &tree,
                };
                for engine in &engines {
                    engine.check(&source, &mut sink);
                }
                let outline = outlined.then(|| Arc::new(Outline::new(path, &tree.root())));
                (sink, outline.into_iter().collect())
            }
            Err(fault) => {
                sink.faults.push(fault);
                (sink, Vec::new())
            }
        }
    }

    fn outlined(root: &Path, path: &Path) -> Result<Arc<Outline>, Fault> {
        let text = Self::read(root, path)?;
        let tree = SupportLang::Python.ast_grep(&text);
        Ok(Arc::new(Outline::new(path, &tree.root())))
    }

    fn graphed(
        &self,
        root: &Path,
        files: &[PathBuf],
        explicit: bool,
        mut outlines: Vec<Arc<Outline>>,
        sink: &mut Sink,
    ) -> Extent {
        let walked: HashSet<&Path> = files.iter().map(PathBuf::as_path).collect();
        if explicit {
            let starts = Self::starts(root, &self.roots, &mut sink.faults);
            let modules = Walk::new(root, &starts);
            sink.faults.extend(modules.faults);
            let unread: Vec<&PathBuf> = modules
                .files
                .iter()
                .filter(|path| !walked.contains(path.as_path()) && self.ancestry.named(path).is_some())
                .collect();
            let read: Vec<Result<Arc<Outline>, Fault>> =
                unread.par_iter().map(|path| Self::outlined(root, path)).collect();
            for outline in read {
                match outline {
                    Ok(outline) => outlines.push(outline),
                    Err(fault) => sink.faults.push(fault),
                }
            }
        }
        let graph = Graph::new(&self.ancestry, &outlines);
        let mut found = Sink::default();
        self.graph.check(&graph, explicit.then_some(&walked), &mut found);
        found.findings.retain(|finding| walked.contains(finding.path.as_path()));
        sink.absorb(found);
        graph.extent()
    }

    fn spawn<'scope>(
        &'scope self,
        scope: &'scope Scope<'scope, '_>,
        root: &'scope Path,
        starts: &'scope [PathBuf],
    ) -> Vec<(&'scope str, ScopedJoinHandle<'scope, (Sink, Duration)>)> {
        self.batch
            .iter()
            .map(|engine| {
                let running = scope.spawn(move || {
                    let started = Instant::now();
                    let mut sink = Sink::default();
                    engine.check(root, starts, &mut sink);
                    (sink, started.elapsed())
                });
                (engine.name(), running)
            })
            .collect()
    }

    fn joined(&self, running: Vec<(&str, ScopedJoinHandle<'_, (Sink, Duration)>)>, timings: &mut Vec<Timing>) -> Sink {
        let mut sink = running.into_iter().fold(Sink::default(), |mut all, (name, running)| {
            if let Ok((one, elapsed)) = running.join() {
                all.absorb(one);
                timings.push(Timing::new(name, elapsed));
            } else {
                all.faults.push(Fault::External {
                    engine: name.to_owned(),
                    message: PANICKED.to_owned(),
                });
            }
            all
        });
        sink.faults.sort_by_cached_key(ToString::to_string);
        sink.faults.dedup();
        sink.findings.retain(|finding| {
            self.external
                .get(&finding.rule)
                .is_some_and(|scope| scope.admits(&finding.path))
        });
        sink
    }

    fn starts(root: &Path, paths: &[PathBuf], faults: &mut Vec<Fault>) -> Vec<PathBuf> {
        paths
            .iter()
            .filter_map(|path| {
                fs::canonicalize(root.join(path))
                    .map_err(|error| Fault::io(path, &error))
                    .and_then(|full| {
                        full.strip_prefix(root)
                            .map(|relative| {
                                let here = relative.as_os_str().is_empty();
                                if here { PathBuf::from(HERE) } else { relative.to_owned() }
                            })
                            .map_err(|_| Fault::Outside { path: path.clone() })
                    })
                    .map_err(|fault| faults.push(fault))
                    .ok()
            })
            .collect()
    }

    /// The run of a loaded configuration under a selection, as of a day, its external rules checked by the batch
    /// engines handed in, its bases resolved also through the environment handed in; the reserved rules always run.
    ///
    /// # Errors
    /// Every fault found: a rule its engine refuses, a scope that does not compile, an exception naming a symbol, an
    /// external rule no batch engine checks.
    pub fn new(
        loaded: &Loaded,
        selection: &Selection,
        today: Date,
        batch: Vec<Box<dyn BatchEngine>>,
        environment: &Environment,
    ) -> Result<Self, Vec<Fault>> {
        let overrides = &loaded.project.config.profiles;
        let declared: BTreeMap<_, _> = ScopeMatcher::profiles(&loaded.catalogues)
            .into_iter()
            .map(|(kind, profile)| match overrides.get(&kind) {
                Some(over) => (kind, profile.tuned(&over.thresholds)),
                None => (kind, profile),
            })
            .collect();
        let rules = || loaded.catalogues.iter().flat_map(|catalogue| &catalogue.rules);
        let mut plan = rules()
            .filter(|rule| selection.admits(rule))
            .fold(Plan::default(), |plan, rule| plan.take(rule, &declared));
        let claimed: Vec<RuleId> = batch.iter().flat_map(|engine| engine.rules()).cloned().collect();
        plan.faults.extend(plan.unclaimed(&claimed));
        let config = &loaded.project.config;
        let file = config.exceptions.clone().unwrap_or_default();
        let ledger = ExceptionLedger::new(&file, &loaded.exceptions, today)
            .map_err(|faults| plan.faults.extend(faults))
            .ok();
        let stamps: Vec<Stamp> = loaded
            .catalogues
            .iter()
            .filter_map(|catalogue| Self::stamp(catalogue).map_err(|fault| plan.faults.push(fault)).ok())
            .collect();
        let [pragma, expired, unused] = [Reserved::Pragma, Reserved::Expired, Reserved::Unused].map(|reserved| {
            reserved
                .rule()
                .map_err(|invalid| plan.faults.push(Fault::Reserved(invalid)))
                .ok()
        });
        plan.faults.extend(Bank::fault());
        let reserved = [&pragma, &expired, &unused];
        let signed = || {
            reserved
                .into_iter()
                .flatten()
                .map(|rule| (rule.id().clone(), rule.tags().clone()))
        };
        let known: BTreeMap<RuleId, Tags> = rules()
            .map(|rule| (rule.id.clone(), rule.tags.clone()))
            .chain(signed())
            .collect();
        let former: BTreeMap<Former, RuleId> = rules()
            .flat_map(|rule| rule.formerly.iter().map(|old| (old.clone(), rule.id.clone())))
            .collect();
        let messages: BTreeMap<RuleId, String> = rules()
            .map(|rule| (rule.id.clone(), rule.message.as_str().to_owned()))
            .chain(
                reserved
                    .into_iter()
                    .flatten()
                    .map(|rule| (rule.id().clone(), rule.message().to_owned())),
            )
            .collect();
        let admitted: Vec<(RuleId, Tags)> = rules()
            .filter(|rule| selection.admits(rule))
            .map(|rule| (rule.id.clone(), rule.tags.clone()))
            .chain(signed())
            .collect();
        match (ledger, pragma, expired, unused) {
            (Some(ledger), Some(pragma), Some(expired), Some(unused)) if plan.faults.is_empty() => {
                let external = std::mem::take(&mut plan.external);
                let profiled = !plan.profile.is_empty();
                let metered = !plan.metric.is_empty();
                let ancestry = Arc::new(Ancestry::new(&loaded.project.root, &config.roots, environment));
                let (mut files, tree, graph) = plan.engines(Arc::clone(&ancestry), &loaded.project.root);
                files.push(Box::new(PragmaEngine::new(&loaded.catalogues)));
                Ok(Self {
                    root: loaded.project.root.clone(),
                    roots: config.roots.clone(),
                    selection: selection.clone(),
                    stamps,
                    files,
                    tree,
                    graph,
                    batch,
                    external,
                    ledger,
                    known,
                    former,
                    messages,
                    admitted,
                    pragma,
                    expired,
                    unused,
                    ancestry,
                    profiled,
                    metered,
                })
            }
            _ => Err(plan.faults),
        }
    }

    /// The report of the paths given, from the project root, or of the configured roots when none is; stamped `at`.
    #[must_use]
    pub fn check(&self, paths: &[PathBuf], at: Timestamp) -> Outcome {
        let mut faults = Vec::new();
        let root = fs::canonicalize(&self.root)
            .map_err(|error| faults.push(Fault::io(&self.root, &error)))
            .unwrap_or_else(|()| self.root.clone());
        let explicit = !paths.is_empty();
        let mut timings = Vec::new();
        let started = Instant::now();
        let starts = Self::starts(&root, if explicit { paths } else { &self.roots }, &mut faults);
        let walk = Walk::new(&root, &starts);
        timings.push(Timing::new(WALK, started.elapsed()));
        faults.extend(walk.faults);
        let (mut sink, outlines, batch) = thread::scope(|scope| {
            let running = self.spawn(scope, &root, &starts);
            let started = Instant::now();
            let (files, outlines) = walk.files.par_iter().map(|path| self.file(&root, path)).reduce(
                || (Sink::default(), Vec::new()),
                |(mut all, mut outlines), (one, more)| {
                    all.absorb(one);
                    outlines.extend(more);
                    (all, outlines)
                },
            );
            timings.push(Timing::new(PARSE, started.elapsed()));
            (files, outlines, self.joined(running, &mut timings))
        });
        sink.absorb(batch);
        let started = Instant::now();
        let graph = self
            .graph
            .judges()
            .then(|| self.graphed(&root, &walk.files, explicit, outlines, &mut sink));
        if graph.is_some() {
            timings.push(Timing::new(GRAPH, started.elapsed()));
        }
        let started = Instant::now();
        walk.dirs.iter().for_each(|dir| self.tree.check(&root, dir, &mut sink));
        timings.push(Timing::new(TREE, started.elapsed()));
        let mut used = self.ledger.unmarked();
        let findings = self.ledger.apply(sink.findings, &mut used);
        let mut reserved = Sink::default();
        self.ledger.license(
            &sink.pragmas,
            &self.known,
            &self.former,
            &self.pragma,
            &mut used,
            &mut reserved,
        );
        self.ledger.expired(&self.expired, &mut reserved);
        if !explicit {
            self.ledger
                .unused(&self.unused, &used, &self.known, &self.admitted, &mut reserved);
        }
        faults.extend(sink.faults);
        let run = law_model::Run {
            tool: env!("CARGO_PKG_VERSION").to_owned(),
            commit: None,
            date: at,
            catalogues: self.stamps.clone(),
            select: self.selection.select.clone(),
            ignore: self.selection.ignore.clone(),
            paths: starts,
            tools: self
                .batch
                .iter()
                .map(|engine| Version {
                    name: engine.name().to_owned(),
                    version: engine.version().to_owned(),
                })
                .collect(),
        };
        let mut census = sink.census;
        (census.project, census.foreign) = self.ancestry.read();
        census.profile = self.profiled.then(|| census.profile.unwrap_or_default());
        census.metric = self.metered.then(|| census.metric.unwrap_or_default());
        let mut report = Report::new(run, findings.into_iter().chain(reserved.findings).collect());
        report.rules = report
            .findings
            .iter()
            .filter_map(|finding| {
                self.messages
                    .get(&finding.rule)
                    .map(|message| (finding.rule.clone(), message.clone()))
            })
            .collect();
        Outcome {
            report,
            faults,
            census,
            graph,
            timings,
        }
    }
}
