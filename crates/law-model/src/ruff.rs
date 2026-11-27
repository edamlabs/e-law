use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::code::Code;
use crate::expression::Expression;

/// A ruff rule, by its code; `message` splits one code among several rules by what ruff says.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuffSpec {
    /// The ruff code.
    pub code: Code,
    /// The expression found in ruff's message of the diagnostics this rule takes; absent, it takes every diagnostic
    /// of its code that no sibling with a `message` takes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<Expression>,
}
