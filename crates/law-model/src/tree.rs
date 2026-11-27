use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::expression::Expression;
use crate::glob::Glob;

/// What each directory matching `dirs` may hold, must not hold and must hold; an entry is a name, a subdirectory's with a trailing `/`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TreeSpec {
    /// The directories looked at, from the project root.
    pub dirs: Vec<Glob>,
    /// The entries allowed; absent, anything is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow: Option<Vec<Glob>>,
    /// The entries banned: any entry matching one is a finding.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deny: Vec<Glob>,
    /// The entries required.
    #[serde(default)]
    pub require: Vec<Glob>,
    /// The entries `name` looks at; absent, every one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub only: Option<Vec<Glob>>,
    /// The expression every entry's name must match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<Expression>,
}
