use std::collections::BTreeMap;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ast_grep_language::{LanguageExt, SupportLang};
use law_config::{Loaded, Selection};
use law_model::{
    Case, CaseKind, Check, Finding, FixPolicy, FixedCase, Glob, Kind, MetricSpec, Need, Profile, ProfileSpec, Rule,
    Safety, Snippet, Trial, Trials, Untested,
};

use crate::ancestry::Ancestry;
use crate::bank::Bank;
use crate::digraph::Graph;
use crate::engine::{CaseEngine, FileEngine, ProjectEngine, Sink};
use crate::environment::Environment;
use crate::fault::{Fault, Feature};
use crate::graph::GraphEngine;
use crate::outline::Outline;
use crate::parity::ParityEngine;
use crate::plan::Plan;
use crate::scope::ScopeMatcher;
use crate::source::Source;

const PASSES: usize = 10;
const UNPROVEN: &str = "no case: an unproven rule is not a rule";
const HOMELESS: &str = "every case sits outside the homes of its kinds: an unproven rule is not a rule";
const ROOTS: [&str; 2] = ["src", "."];

/// What proves a rule: its file engines over it alone and the resolver of their bases, forgotten before each text so
/// every case is a world of its own; the batch engine that checks it, within its scope; the graph engine over the
/// case's files alone, a project whose source roots are `src` then the project root; or the parity engine over the
/// case's files alone, which are all it lists.
enum Prover {
    Files(Vec<Box<dyn FileEngine>>, Arc<Ancestry>),
    Case(Arc<dyn CaseEngine>, ScopeMatcher),
    Graph(GraphEngine, Ancestry),
    Parity(ParityEngine),
}

/// A rule ready to be proven, and, for a `profile` rule, the homes of its kinds, one of which a case must sit in.
struct Proven {
    rule: Rule,
    prover: Prover,
    homes: Option<Vec<Glob>>,
}

/// What `law test` found: the trials, and the faults met running them.
#[derive(Debug)]
pub struct Proof {
    /// The trials.
    pub trials: Trials,
    /// The faults, in rule order.
    pub faults: Vec<Fault>,
}

/// The runner of `law test`: every selected rule against its own cases, alone, with no exception and no pragma
/// licensing; in memory, or through its batch engine.
pub struct TrialRunner {
    proven: Vec<Proven>,
    untested: Vec<Untested>,
}

impl Proven {
    fn findings(&self, path: &Path, text: &str, faults: &mut Vec<Fault>) -> Vec<Finding> {
        let mut sink = Sink::default();
        match &self.prover {
            Prover::Files(engines, ancestry) => {
                ancestry.clear();
                let tree = SupportLang::Python.ast_grep(text);
                let source = Source {
                    path,
                    text,
                    root: &tree,
                };
                engines
                    .iter()
                    .filter(|engine| engine.admits(path))
                    .for_each(|engine| engine.check(&source, &mut sink));
            }
            Prover::Case(engine, scope) if scope.admits(path) => engine.case(&self.rule.id, path, text, &mut sink),
            Prover::Case(..) => {}
            Prover::Graph(engine, ancestry) => Self::graphed(engine, ancestry, [(path, text)], &mut sink),
            Prover::Parity(engine) => engine.case(&[(path, text)], &mut sink),
        }
        faults.extend(sink.faults);
        sink.findings
    }

    fn graphed<'a>(
        engine: &GraphEngine,
        ancestry: &Ancestry,
        files: impl IntoIterator<Item = (&'a Path, &'a str)>,
        sink: &mut Sink,
    ) {
        let outlines: Vec<Arc<Outline>> = files
            .into_iter()
            .map(|(path, text)| Arc::new(Outline::new(path, &SupportLang::Python.ast_grep(text).root())))
            .collect();
        engine.check(&Graph::new(ancestry, &outlines), None, sink);
    }

    fn case(&self, case: &Case, faults: &mut Vec<Fault>) -> Vec<Finding> {
        match (&case.snippet, &self.prover) {
            (Snippet::Code(code), _) => {
                let path = case.path.clone().unwrap_or_else(|| self.rule.scope.sample());
                self.findings(&path, code, faults)
            }
            (Snippet::Files(files), Prover::Graph(engine, ancestry)) => {
                let mut sink = Sink::default();
                let files = files.iter().map(|(path, text)| (path.as_path(), text.as_str()));
                Self::graphed(engine, ancestry, files, &mut sink);
                faults.extend(sink.faults);
                sink.findings
            }
            (Snippet::Files(files), Prover::Parity(engine)) => {
                let mut sink = Sink::default();
                let files: Vec<(&Path, &str)> = files
                    .iter()
                    .map(|(path, text)| (path.as_path(), text.as_str()))
                    .collect();
                engine.case(&files, &mut sink);
                faults.extend(sink.faults);
                sink.findings
            }
            (Snippet::Files(_), _) => {
                faults.push(Fault::Unsupported {
                    rule: self.rule.id.clone(),
                    feature: Feature::Files,
                });
                Vec::new()
            }
        }
    }

    fn trial(&self, kind: CaseKind, index: usize, detail: Option<String>) -> Trial {
        Trial {
            rule: self.rule.id.clone(),
            kind,
            index: Some(index + 1),
            pass: detail.is_none(),
            detail,
        }
    }

    fn unproven(&self, kind: CaseKind, why: &str) -> Trial {
        Trial {
            rule: self.rule.id.clone(),
            kind,
            index: None,
            pass: false,
            detail: Some(why.to_owned()),
        }
    }

    fn homeless(&self) -> bool {
        let Some(homes) = &self.homes else {
            return false;
        };
        let place = |path: &Option<PathBuf>| path.clone().unwrap_or_else(|| self.rule.scope.sample());
        let cases = &self.rule.cases;
        let mut paths = cases
            .valid
            .iter()
            .chain(&cases.invalid)
            .map(|case| place(&case.path))
            .chain(cases.fixed.iter().map(|case| place(&case.path)));
        !paths.any(|path| homes.iter().any(|glob| glob.matches(&path)))
    }

    fn found(findings: &[Finding]) -> Option<String> {
        findings.first().map(|first| {
            format!(
                "found {} (the first at {}:{})",
                findings.len(),
                first.span.start.line,
                first.span.start.column
            )
        })
    }

    fn fixed(&self, path: &Path, code: &str, faults: &mut Vec<Fault>) -> String {
        let fixed = (0..PASSES).try_fold(code.to_owned(), |text, _| {
            let findings = self.findings(path, &text, faults);
            let (next, applied) = law_fix::patch(path, &text, &findings, Safety::Unsafe);
            faults.extend(applied.faults.into_iter().map(Fault::Fix));
            if applied.fixed.is_empty() {
                ControlFlow::Break(text)
            } else {
                ControlFlow::Continue(next)
            }
        });
        match fixed {
            ControlFlow::Break(text) | ControlFlow::Continue(text) => text,
        }
    }

    fn proof(&self, case: &FixedCase, faults: &mut Vec<Fault>) -> Option<String> {
        let path = case.path.clone().unwrap_or_else(|| self.rule.scope.sample());
        let text = self.fixed(&path, &case.code, faults);
        (text != case.expect)
            .then(|| format!("the fix gives {text:?}, expected {:?}", case.expect))
            .or_else(|| Self::found(&self.findings(&path, &text, faults)).map(|found| format!("{found} after the fix")))
    }

    fn run(&self, faults: &mut Vec<Fault>) -> Vec<Trial> {
        let cases = &self.rule.cases;
        let valid: Vec<Trial> = cases
            .valid
            .iter()
            .enumerate()
            .map(|(index, case)| {
                let findings = self.case(case, faults);
                let detail = Self::found(&findings).map(|found| format!("expected no finding, {found}"));
                self.trial(CaseKind::Valid, index, detail)
            })
            .collect();
        let invalid: Vec<Trial> = cases
            .invalid
            .iter()
            .enumerate()
            .map(|(index, case)| {
                let findings = self.case(case, faults);
                let detail = findings.is_empty().then(|| "expected a finding, found none".to_owned());
                self.trial(CaseKind::Invalid, index, detail)
            })
            .collect();
        let fixed: Vec<Trial> = cases
            .fixed
            .iter()
            .enumerate()
            .map(|(index, case)| {
                let detail = self.proof(case, faults);
                self.trial(CaseKind::Fixed, index, detail)
            })
            .collect();
        let fixable = matches!(self.rule.fix, FixPolicy::Safe | FixPolicy::Unsafe);
        let missing = [
            cases
                .invalid
                .is_empty()
                .then(|| self.unproven(CaseKind::Invalid, UNPROVEN)),
            (fixable && cases.fixed.is_empty()).then(|| self.unproven(CaseKind::Fixed, UNPROVEN)),
            (!cases.invalid.is_empty() && self.homeless()).then(|| self.unproven(CaseKind::Invalid, HOMELESS)),
        ];
        valid
            .into_iter()
            .chain(invalid)
            .chain(fixed)
            .chain(missing.into_iter().flatten())
            .collect()
    }
}

impl TrialRunner {
    fn homes(rule: &Rule, profiles: &BTreeMap<Kind, Profile>) -> Option<Vec<Glob>> {
        match &rule.check {
            Check::Profile(ProfileSpec { kinds, .. }) | Check::Metric(MetricSpec { kinds, .. }) => Some(
                kinds
                    .iter()
                    .filter_map(|kind| profiles.get(kind))
                    .flat_map(|profile| profile.home.iter().cloned())
                    .collect(),
            ),
            _ => None,
        }
    }

    fn prover(rule: &Rule, plan: Plan, cases: &[Arc<dyn CaseEngine>]) -> Result<Prover, Vec<Fault>> {
        if !plan.faults.is_empty() {
            return Err(plan.faults);
        }
        let graphed = matches!(rule.check, Check::Graph(_));
        match plan.external.get(&rule.id) {
            None if matches!(rule.check, Check::Parity(_)) => {
                Ok(Prover::Parity(ParityEngine::new(plan.parity, PathBuf::new())))
            }
            None if graphed => {
                let roots: Vec<PathBuf> = ROOTS.iter().map(PathBuf::from).collect();
                let ancestry = Ancestry::new(Path::new(""), &roots, &Environment::default());
                Ok(Prover::Graph(plan.engines(Arc::default(), Path::new("")).2, ancestry))
            }
            None => {
                let ancestry = Arc::new(Ancestry::default());
                Ok(Prover::Files(
                    plan.engines(Arc::clone(&ancestry), Path::new("")).0,
                    ancestry,
                ))
            }
            Some(scope) => cases
                .iter()
                .find(|engine| engine.rules().contains(&rule.id))
                .map(|engine| Prover::Case(Arc::clone(engine), scope.clone()))
                .ok_or_else(|| vec![Fault::NoEngine { rule: rule.id.clone() }]),
        }
    }

    /// Every rule the selection admits, each compiled alone, an external one proven by the case engine handed in that
    /// checks it; `tree` and `ty` rules set apart as untested.
    ///
    /// # Errors
    /// Every fault found compiling the rules, every external rule no case engine checks, and a bank that does not
    /// decode.
    pub fn new(loaded: &Loaded, selection: &Selection, cases: Vec<Box<dyn CaseEngine>>) -> Result<Self, Vec<Fault>> {
        let profiles = ScopeMatcher::profiles(&loaded.catalogues);
        let cases: Vec<Arc<dyn CaseEngine>> = cases.into_iter().map(Arc::from).collect();
        let admitted = loaded
            .catalogues
            .iter()
            .flat_map(|catalogue| &catalogue.rules)
            .filter(|rule| selection.admits(rule));
        let (untested, proven): (Vec<&Rule>, Vec<&Rule>) = admitted.partition(|rule| Need::of(&rule.check).is_some());
        let mut faults: Vec<Fault> = Bank::fault().into_iter().collect();
        let proven = proven
            .into_iter()
            .filter_map(|rule| {
                Self::prover(rule, Plan::default().take(rule, &profiles), &cases)
                    .map(|prover| Proven {
                        rule: rule.clone(),
                        prover,
                        homes: Self::homes(rule, &profiles),
                    })
                    .map_err(|errors| faults.extend(errors))
                    .ok()
            })
            .collect();
        if faults.is_empty() {
            Ok(Self {
                proven,
                untested: untested
                    .into_iter()
                    .filter_map(|rule| {
                        Need::of(&rule.check).map(|need| Untested {
                            rule: rule.id.clone(),
                            need,
                        })
                    })
                    .collect(),
            })
        } else {
            Err(faults)
        }
    }

    /// Every rule against its cases; a rule with no `invalid` case, or a fixable one with no `fixed` case, fails.
    #[must_use]
    pub fn run(&self) -> Proof {
        let mut faults = Vec::new();
        let trials = self.proven.iter().flat_map(|proven| proven.run(&mut faults)).collect();
        Proof {
            trials: Trials::new(self.proven.len(), trials, self.untested.clone()),
            faults,
        }
    }
}
