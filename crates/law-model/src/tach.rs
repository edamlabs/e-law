use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A tach rule, by tach's name for the kind of diagnostic; absent, every tach diagnostic no sibling with a code takes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TachSpec {
    /// The kind, as tach names it: `UndeclaredDependency`, `LayerViolation`, `CircularDependency`…
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}
