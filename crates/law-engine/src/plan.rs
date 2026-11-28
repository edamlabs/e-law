use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use law_model::{Check, Kind, Profile, Rule, RuleId};

use crate::ancestry::Ancestry;
use crate::engine::FileEngine;
use crate::fault::{Fault, Feature};
use crate::graph::{GraphEngine, GraphRule};
use crate::members::{MembersEngine, MembersRule};
use crate::metric::{MetricEngine, MetricRule};
use crate::parity::{ParityEngine, ParityRule};
use crate::pattern::{PatternEngine, PatternRule};
use crate::profile::{ProfileEngine, ProfileRule};
use crate::relocate::Relocator;
use crate::scope::ScopeMatcher;
use crate::shape::{ShapeEngine, ShapeRule};
use crate::tree::{TreeEngine, TreeRule};

/// Rules compiled into their engines' own rules, and every fault met doing it.
#[derive(Default)]
pub(crate) struct Plan {
    pub(crate) tree: Vec<TreeRule>,
    pub(crate) shape: Vec<ShapeRule>,
    pub(crate) pattern: Vec<PatternRule>,
    pub(crate) members: Vec<MembersRule>,
    pub(crate) profile: Vec<ProfileRule>,
    pub(crate) metric: Vec<MetricRule>,
    pub(crate) graph: Vec<GraphRule>,
    pub(crate) parity: Vec<ParityRule>,
    pub(crate) external: BTreeMap<RuleId, ScopeMatcher>,
    pub(crate) faults: Vec<Fault>,
}

impl Plan {
    /// The plan with one more rule compiled, or its faults.
    pub(crate) fn take(mut self, rule: &Rule, profiles: &BTreeMap<Kind, Profile>) -> Self {
        let planned = ScopeMatcher::new(rule, profiles)
            .map_err(|fault| vec![fault])
            .and_then(|scope| match &rule.check {
                Check::Tree(spec) => {
                    self.tree.push(TreeRule::new(rule, spec, scope));
                    Ok(())
                }
                Check::Shape(spec) => {
                    self.shape.push(ShapeRule::new(rule, spec, scope));
                    Ok(())
                }
                Check::Pattern(spec) => PatternRule::new(rule, spec, scope).map(|planned| self.pattern.push(planned)),
                Check::Members(spec) => {
                    self.members.push(MembersRule::new(rule, spec, scope));
                    Ok(())
                }
                Check::Profile(spec) => {
                    self.profile.push(ProfileRule::new(rule, spec, scope, profiles));
                    Ok(())
                }
                Check::Metric(spec) => MetricRule::new(rule, spec, scope, profiles)
                    .map(|planned| self.metric.push(planned))
                    .map_err(|fault| vec![fault]),
                Check::Graph(spec) => {
                    self.graph.push(GraphRule::new(rule, spec, scope));
                    Ok(())
                }
                Check::Parity(spec) => {
                    self.parity.push(ParityRule::new(rule, spec, scope));
                    Ok(())
                }
                Check::Ruff(_) | Check::Ty(_) | Check::Tach(_) => {
                    self.external.insert(rule.id.clone(), scope);
                    Ok(())
                }
                _ => Err(vec![Fault::Unsupported {
                    rule: rule.id.clone(),
                    feature: Feature::Engine,
                }]),
            });
        self.faults.extend(planned.err().into_iter().flatten());
        self
    }

    /// What moves the modules the metric rules find over their measure, for those whose fix does.
    pub(crate) fn relocator(&self) -> Relocator {
        Relocator::new(self.metric.iter().filter_map(MetricRule::relocating).collect())
    }

    /// The rules no batch engine checks, each a fault.
    pub(crate) fn unclaimed(&self, claimed: &[RuleId]) -> Vec<Fault> {
        self.external
            .keys()
            .filter(|rule| !claimed.contains(rule))
            .map(|rule| Fault::NoEngine { rule: rule.clone() })
            .collect()
    }

    /// The file engines over the planned file rules, bases resolved by the resolver handed in and entries listed from
    /// the project root handed in, the tree engine over the tree rules, and the graph engine over the graph rules.
    pub(crate) fn engines(
        self,
        ancestry: Arc<Ancestry>,
        root: &Path,
    ) -> (Vec<Box<dyn FileEngine>>, TreeEngine, GraphEngine) {
        let files: Vec<Box<dyn FileEngine>> = vec![
            Box::new(PatternEngine::new(self.pattern)),
            Box::new(ShapeEngine::new(self.shape)),
            Box::new(ProfileEngine::new(self.profile, Arc::clone(&ancestry))),
            Box::new(MetricEngine::new(self.metric)),
            Box::new(MembersEngine::new(self.members, ancestry)),
            Box::new(ParityEngine::new(self.parity, root.to_owned())),
        ];
        (files, TreeEngine::new(self.tree), GraphEngine::new(self.graph))
    }
}
