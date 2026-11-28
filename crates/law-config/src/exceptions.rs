use law_model::Exception;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// An exceptions file: its `[[exception]]` tables.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Exceptions {
    /// The exceptions declared.
    #[serde(default)]
    pub exception: Vec<Exception>,
}
