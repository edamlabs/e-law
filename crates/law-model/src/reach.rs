use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What an exception does where it matches: drop the rule's findings, or license its inline pragmas.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Reach {
    /// Every finding of the rule in the matching paths is dropped.
    #[default]
    File,
    /// Nothing is dropped; inline pragmas naming the rule in the matching paths are licensed.
    Pragma,
}
