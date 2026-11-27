use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use law_model::{Kind, RuleId, Selector};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

use crate::fault::Fault;

/// The file name of a project's own configuration.
pub(crate) const LAW: &str = "law.toml";
/// The file name of a Python project's manifest, whose `[tool.law]` is a configuration.
pub(crate) const PYPROJECT: &str = "pyproject.toml";

/// A catalogue to load: a built-in name, or a directory from the project root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum Source {
    /// A catalogue the tool carries, by name.
    Builtin(String),
    /// A catalogue directory.
    Dir(PathBuf),
}

/// The values a project overrides in a profile.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Overrides {
    /// Thresholds, by the name the profile declares.
    #[serde(default)]
    pub thresholds: BTreeMap<String, u32>,
}

/// A project's configuration: `law.toml`, or `[tool.law]` in `pyproject.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// The directories checked, from the project root.
    #[serde(default = "Config::here")]
    pub roots: Vec<PathBuf>,
    /// The catalogues loaded, in order; a later one extends the earlier.
    #[serde(default)]
    pub catalogues: Vec<Source>,
    /// The rules selected.
    #[serde(default = "Config::all")]
    pub select: Vec<Selector>,
    /// The rules ignored; ignoring wins over selecting.
    #[serde(default)]
    pub ignore: Vec<Selector>,
    /// The exceptions file, from the project root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exceptions: Option<PathBuf>,
    /// Per profile kind, the values overridden.
    #[serde(default)]
    pub profiles: BTreeMap<Kind, Overrides>,
}

#[derive(Deserialize)]
struct Pyproject {
    #[serde(default)]
    tool: Option<Tool>,
}

#[derive(Deserialize)]
struct Tool {
    #[serde(default)]
    law: Option<Config>,
}

impl Config {
    fn here() -> Vec<PathBuf> {
        vec![PathBuf::from(".")]
    }

    fn all() -> Vec<Selector> {
        vec![Selector::All]
    }

    /// The configuration a file holds: `[tool.law]` when it is a `pyproject.toml`, the whole file otherwise.
    ///
    /// # Errors
    /// When the text does not decode into a configuration.
    pub fn parse(text: &str, path: &Path) -> Result<Option<Self>, Fault> {
        match path.file_name().and_then(|name| name.to_str()) {
            Some(PYPROJECT) => toml::from_str::<Pyproject>(text)
                .map(|pyproject| pyproject.tool.and_then(|tool| tool.law))
                .map_err(|error| Fault::toml(path, text, &error)),
            _ => toml::from_str(text)
                .map(Some)
                .map_err(|error| Fault::toml(path, text, &error)),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            roots: Self::here(),
            catalogues: Vec::new(),
            select: Self::all(),
            ignore: Vec::new(),
            exceptions: None,
            profiles: BTreeMap::new(),
        }
    }
}

impl From<String> for Source {
    fn from(text: String) -> Self {
        if RuleId::word(&text) {
            Self::Builtin(text)
        } else {
            Self::Dir(PathBuf::from(text))
        }
    }
}

impl From<Source> for String {
    fn from(source: Source) -> Self {
        source.to_string()
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Builtin(name) => f.write_str(name),
            Self::Dir(dir) => {
                let text = dir.to_string_lossy();
                let slash = if RuleId::word(&text) { "/" } else { "" };
                write!(f, "{text}{slash}")
            }
        }
    }
}

impl JsonSchema for Source {
    fn schema_name() -> Cow<'static, str> {
        "Source".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "minLength": 1,
            "description": "A built-in catalogue's name (one lowercase word), or a directory from the project root.",
        })
    }
}
