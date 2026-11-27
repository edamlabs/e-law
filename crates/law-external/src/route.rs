use std::collections::BTreeSet;

use law_model::{Check, Code, RuffSpec, Rule, RuleId, TachSpec, TySpec};

/// A loaded rule an external tool's diagnostics may go to, and whether the run selects it.
#[derive(Debug, Clone)]
struct Target<S> {
    rule: Rule,
    spec: S,
    selected: bool,
}

/// Where ruff's diagnostics go: to the rules of their code whose `message` ruff's message holds, else to those of
/// their code without one; among the loaded rules, kept only when selected.
#[derive(Debug, Clone)]
pub struct RuffRoute {
    targets: Vec<Target<RuffSpec>>,
}

/// Where ty's diagnostics go: to the rules naming their ty rule, else to those naming none; among the loaded rules,
/// kept only when selected.
#[derive(Debug, Clone)]
pub struct TyRoute {
    targets: Vec<Target<TySpec>>,
}

/// Where tach's diagnostics go: to the rules naming their kind, else to those naming none; among the loaded rules,
/// kept only when selected.
#[derive(Debug, Clone)]
pub struct TachRoute {
    targets: Vec<Target<TachSpec>>,
}

impl<S> Target<S> {
    fn chosen<'a>(targets: impl Iterator<Item = &'a Self>) -> Vec<&'a Rule>
    where
        S: 'a,
    {
        targets
            .filter(|target| target.selected)
            .map(|target| &target.rule)
            .collect()
    }

    fn selected(targets: &[Self]) -> Vec<&Rule> {
        Self::chosen(targets.iter())
    }
}

impl RuffRoute {
    /// The route over the loaded rules, `admits` telling the selected ones.
    pub fn new<'a>(rules: impl IntoIterator<Item = &'a Rule>, admits: impl Fn(&Rule) -> bool) -> Self {
        let targets = rules
            .into_iter()
            .filter_map(|rule| match &rule.check {
                Check::Ruff(spec) => Some(Target {
                    rule: rule.clone(),
                    spec: spec.clone(),
                    selected: admits(rule),
                }),
                _ => None,
            })
            .collect();
        Self { targets }
    }

    /// The selected rules.
    #[must_use]
    pub fn rules(&self) -> Vec<&Rule> {
        Target::selected(&self.targets)
    }

    /// The codes the selected rules name, once each, in order.
    #[must_use]
    pub fn codes(&self) -> Vec<&Code> {
        let codes: BTreeSet<&Code> = self
            .targets
            .iter()
            .filter(|target| target.selected)
            .map(|target| &target.spec.code)
            .collect();
        codes.into_iter().collect()
    }

    /// The selected rules a diagnostic of a code, saying a message, goes to.
    #[must_use]
    pub fn route(&self, code: &str, message: &str) -> Vec<&Rule> {
        let coded = || {
            self.targets
                .iter()
                .filter(move |target| target.spec.code.as_str() == code)
        };
        let found = || {
            coded().filter(|target| {
                target
                    .spec
                    .message
                    .as_ref()
                    .is_some_and(|expression| expression.finds(message))
            })
        };
        if found().next().is_some() {
            Target::chosen(found())
        } else {
            Target::chosen(coded().filter(|target| target.spec.message.is_none()))
        }
    }

    /// The ruff code of a loaded rule.
    #[must_use]
    pub fn code(&self, rule: &RuleId) -> Option<&Code> {
        self.targets
            .iter()
            .find(|target| &target.rule.id == rule)
            .map(|target| &target.spec.code)
    }
}

impl TyRoute {
    /// The route over the loaded rules, `admits` telling the selected ones.
    pub fn new<'a>(rules: impl IntoIterator<Item = &'a Rule>, admits: impl Fn(&Rule) -> bool) -> Self {
        let targets = rules
            .into_iter()
            .filter_map(|rule| match &rule.check {
                Check::Ty(spec) => Some(Target {
                    rule: rule.clone(),
                    spec: spec.clone(),
                    selected: admits(rule),
                }),
                _ => None,
            })
            .collect();
        Self { targets }
    }

    /// The selected rules.
    #[must_use]
    pub fn rules(&self) -> Vec<&Rule> {
        Target::selected(&self.targets)
    }

    /// The selected rules a diagnostic of a ty rule goes to.
    #[must_use]
    pub fn route(&self, name: &str) -> Vec<&Rule> {
        let named = || {
            self.targets
                .iter()
                .filter(move |target| target.spec.rule.as_deref() == Some(name))
        };
        if named().next().is_some() {
            Target::chosen(named())
        } else {
            Target::chosen(self.targets.iter().filter(|target| target.spec.rule.is_none()))
        }
    }
}

impl TachRoute {
    /// The route over the loaded rules, `admits` telling the selected ones.
    pub fn new<'a>(rules: impl IntoIterator<Item = &'a Rule>, admits: impl Fn(&Rule) -> bool) -> Self {
        let targets = rules
            .into_iter()
            .filter_map(|rule| match &rule.check {
                Check::Tach(spec) => Some(Target {
                    rule: rule.clone(),
                    spec: spec.clone(),
                    selected: admits(rule),
                }),
                _ => None,
            })
            .collect();
        Self { targets }
    }

    /// The selected rules.
    #[must_use]
    pub fn rules(&self) -> Vec<&Rule> {
        Target::selected(&self.targets)
    }

    /// The kinds the selected rules name, none for a rule that takes every kind.
    #[must_use]
    pub fn codes(&self) -> Vec<Option<&str>> {
        self.targets
            .iter()
            .filter(|target| target.selected)
            .map(|target| target.spec.code.as_deref())
            .collect()
    }

    /// The selected rules a diagnostic of a kind goes to.
    #[must_use]
    pub fn route(&self, kind: &str) -> Vec<&Rule> {
        let named = || {
            self.targets
                .iter()
                .filter(move |target| target.spec.code.as_deref() == Some(kind))
        };
        if named().next().is_some() {
            Target::chosen(named())
        } else {
            Target::chosen(self.targets.iter().filter(|target| target.spec.code.is_none()))
        }
    }
}
