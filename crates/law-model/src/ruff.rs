use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::code::Code;
use crate::expression::Expression;
use crate::setting::Setting;

const EQUALS: &str = " = ";

/// A ruff rule, by its code; `message` splits one code among several rules by what ruff says; `settings` are the
/// options it runs ruff with.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuffSpec {
    /// The ruff code.
    pub code: Code,
    /// The expression found in ruff's message of the diagnostics this rule takes; absent, it takes every diagnostic
    /// of its code that no sibling with a `message` takes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<Expression>,
    /// The ruff options this rule runs with, by their dotted key (`lint.flake8-tidy-imports.banned-api`): each one
    /// replaces whatever the project sets for that key, as ruff's `--config` does. Rules whose settings differ run in
    /// separate ruff processes.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub settings: BTreeMap<String, Setting>,
}

impl RuffSpec {
    /// The `--config` arguments its settings make, one per key, in key order: `<key> = <value>`.
    #[must_use]
    pub fn overrides(&self) -> Vec<String> {
        self.settings
            .iter()
            .map(|(key, value)| format!("{key}{EQUALS}{}", value.toml()))
            .collect()
    }
}
