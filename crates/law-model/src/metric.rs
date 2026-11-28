use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::invalid::{Invalid, Refusal};
use crate::profile::Kind;

/// What a `metric` rule counts in a module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Measure {
    /// Code lines: not blank, not comment-only; a docstring's non-blank lines count.
    Lines,
    /// Top-level classes.
    Classes,
    /// Top-level classes whose name does not end with the profile's suffix.
    Companions,
    /// Top-level functions.
    Functions,
    /// The most methods one top-level class holds.
    Methods,
}

/// The move a `metric` rule's fix makes of a module that is over its measure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Relocation {
    /// The module becomes a package: `<dir>/<name>.py` moves to `<dir>/<name>/<kind>.py`, `<kind>` the kind whose
    /// home the module sits in; every import of it follows.
    Package,
}

/// A `metric` check as written: the spec before it is checked.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Metered {
    /// The kinds measured.
    #[schemars(length(min = 1))]
    pub kinds: Vec<Kind>,
    /// What is counted.
    pub measure: Measure,
    /// The name of the profile threshold the count may not pass.
    #[schemars(length(min = 1))]
    pub over: String,
    /// The move the rule's fix makes; a rule with a `safe` or `unsafe` fix names it, a rule with none does not.
    #[serde(default)]
    pub relocate: Option<Relocation>,
}

/// What a `metric` rule judges: a count over every module in the homes of its kinds, against the threshold of the
/// kind's profile it names.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "Metered")]
pub struct MetricSpec {
    /// The kinds measured.
    #[schemars(length(min = 1))]
    pub kinds: Vec<Kind>,
    /// What is counted.
    pub measure: Measure,
    /// The name of the profile threshold the count may not pass.
    #[schemars(length(min = 1))]
    pub over: String,
    /// The move the rule's fix makes; a rule with a `safe` or `unsafe` fix names it, a rule with none does not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relocate: Option<Relocation>,
}

impl Measure {
    /// The name it is written by.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Lines => "lines",
            Self::Classes => "classes",
            Self::Companions => "companions",
            Self::Functions => "functions",
            Self::Methods => "methods",
        }
    }
}

impl fmt::Display for Measure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl TryFrom<Metered> for MetricSpec {
    type Error = Invalid;

    fn try_from(metered: Metered) -> Result<Self, Self::Error> {
        match metered {
            Metered { kinds, .. } if kinds.is_empty() => Err(Invalid::new(
                Refusal::EmptyKinds,
                "a metric check names at least one kind",
            )),
            Metered { over, .. } if over.is_empty() => Err(Invalid::new(
                Refusal::EmptyText,
                "a metric check names the threshold it is over",
            )),
            Metered {
                kinds,
                measure,
                over,
                relocate,
            } => Ok(Self {
                kinds,
                measure,
                over,
                relocate,
            }),
        }
    }
}
