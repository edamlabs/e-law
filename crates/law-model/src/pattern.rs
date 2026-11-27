use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// An ast-grep rule in ast-grep's own grammar, carried verbatim and validated by the engine at load.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PatternSpec {
    /// The ast-grep rule.
    pub rule: Value,
    /// The constraints on its metavariables.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints: Option<Value>,
    /// The utility rules it refers to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub utils: Option<Value>,
    /// The transforms of its metavariables.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<Value>,
    /// The ast-grep fix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix: Option<Value>,
}
