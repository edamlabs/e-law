use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// How hard a finding blocks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    /// Fails the run.
    #[default]
    Error,
    /// Reported, never fails the run.
    Warning,
}
