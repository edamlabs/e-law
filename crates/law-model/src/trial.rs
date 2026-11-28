use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::check::Check;
use crate::id::RuleId;

/// Which list of a rule's cases a trial comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum CaseKind {
    /// A snippet the rule must pass.
    Valid,
    /// A snippet the rule must catch.
    Invalid,
    /// A snippet the rule's fix must resolve.
    Fixed,
}

/// One rule proven against one of its cases; with no index, the rule lacks a case it must have.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Trial {
    /// The rule.
    pub rule: RuleId,
    /// The list the case comes from.
    pub kind: CaseKind,
    /// The case's 1-based place in its list; none when the list lacks a case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
    /// Whether the rule held.
    pub pass: bool,
    /// What happened, when it did not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// What a rule's engine needs that a case, a lone file, does not give.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Need {
    /// A directory to read entries from: `tree`.
    Directory,
    /// A Python environment to resolve imports in: `ty`.
    Environment,
    /// A project, its `tach.toml` and its modules: `tach`.
    Project,
}

/// A rule `law test` does not prove, and why.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Untested {
    /// The rule.
    pub rule: RuleId,
    /// What its engine needs.
    pub need: Need,
}

/// A `law test` run: every trial in order, the rules it could not test, and the counts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Trials {
    /// The rules tested.
    pub rules: usize,
    /// The cases run.
    pub cases: usize,
    /// The trials that failed.
    pub failed: usize,
    /// The rules whose engine `law test` does not prove yet, and why.
    pub untested: Vec<Untested>,
    /// Every trial, in rule then case order.
    pub trials: Vec<Trial>,
}

impl CaseKind {
    /// The name it is written by.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::Invalid => "invalid",
            Self::Fixed => "fixed",
        }
    }
}

impl Need {
    /// What a check's engine needs that a lone file does not give: a `tree` rule a directory, a `ty` rule an
    /// environment, a `tach` rule a project; nothing for the others.
    #[must_use]
    pub fn of(check: &Check) -> Option<Self> {
        match check {
            Check::Tree(_) => Some(Self::Directory),
            Check::Ty(_) => Some(Self::Environment),
            Check::Tach(_) => Some(Self::Project),
            _ => None,
        }
    }

    /// Why a rule of this need goes untested, in words.
    #[must_use]
    pub fn why(self) -> &'static str {
        match self {
            Self::Directory => "needs a directory",
            Self::Environment => "needs an environment",
            Self::Project => "needs a project",
        }
    }
}

impl Trials {
    /// The trials of `rules` rules, counted; the untested rules listed apart.
    #[must_use]
    pub fn new(rules: usize, trials: Vec<Trial>, untested: Vec<Untested>) -> Self {
        Self {
            rules,
            cases: trials.iter().filter(|trial| trial.index.is_some()).count(),
            failed: trials.iter().filter(|trial| !trial.pass).count(),
            untested,
            trials,
        }
    }
}
