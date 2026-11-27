use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::tags::Tags;

/// What a catalogue declares stays prose: the labels of the law it belongs to, and why no engine checks it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Prose {
    /// The labels it sits under, as a rule's.
    pub tags: Tags,
    /// One line: what no engine can check, and why.
    pub text: String,
}
