use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A ty rule, by ty's name for it; absent, every ty diagnostic no sibling with a name takes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TySpec {
    /// The ty rule's name, which ty does not keep stable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
}
