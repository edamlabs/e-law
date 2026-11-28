use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::profile::Profile;
use crate::prose::Prose;
use crate::rule::Rule;

/// A named, versioned set of rules and profiles, and what it declares stays prose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Catalogue {
    /// Its name.
    pub name: String,
    /// Its version.
    pub version: String,
    /// Its rules.
    pub rules: Vec<Rule>,
    /// Its profiles.
    pub profiles: Vec<Profile>,
    /// What stays prose, no engine checking it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prose: Vec<Prose>,
}
