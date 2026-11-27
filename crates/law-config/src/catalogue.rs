use law_model::Prose;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The file name of a catalogue's manifest.
pub(crate) const MANIFEST: &str = "catalogue.toml";
/// The directory of a catalogue's rule files.
pub(crate) const RULES: &str = "rules";
/// The directory of a catalogue's profile files.
pub(crate) const PROFILES: &str = "profiles";
/// The extension of a catalogue's YAML files.
pub(crate) const YAML: &str = "yml";

/// A catalogue's `catalogue.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Its name.
    pub name: String,
    /// Its version.
    pub version: String,
    /// What it holds.
    pub description: String,
    /// What stays prose, no engine checking it.
    #[serde(default)]
    pub prose: Vec<Prose>,
}
