use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::glob::Glob;

/// What a `graph` rule judges of the project's import graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum GraphAspect {
    /// Modules that import one another round, or a module that imports itself.
    Cycle,
    /// A module no other module imports, none of the entries.
    Orphan,
}

/// What a `graph` rule judges: one aspect of the import graph of every module under the roots.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphSpec {
    /// What is judged.
    pub aspect: GraphAspect,
    /// The modules nothing need import, the doors: `orphan` never judges them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<Glob>,
    /// Whether an import under `if TYPE_CHECKING:` closes a cycle.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub typing: bool,
}

impl GraphAspect {
    /// The name it is written by.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Cycle => "cycle",
            Self::Orphan => "orphan",
        }
    }
}

impl fmt::Display for GraphAspect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}
