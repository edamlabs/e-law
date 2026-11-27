use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

use crate::glob::Glob;
use crate::invalid::{Invalid, Refusal};
use crate::shape::Template;

/// What a class's names are: the names its body binds, or the members of an enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Of {
    /// The names bound or annotated in the class body; methods, nested classes and dunders are no name.
    Attributes,
    /// The valued bindings of an enum's body, compared in lowercase with the entries.
    Members,
}

/// How the names and the entries must stand to one another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Relation {
    /// Every name has an entry and every entry a name.
    Equal,
    /// Every name has an entry.
    NamesInEntries,
    /// Every entry has a name.
    EntriesInNames,
}

/// The exact name of a class: a template filled for the file, with no glob metacharacter in it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Called(Template);

/// The classes that declare names, and which names they declare.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Names {
    /// The name of the top-level class that declares them, a template filled for the file: `{Dir}State` in
    /// `engines/state.py` is `EnginesState`; with no placeholder, exactly that name.
    pub class: Called,
    /// What of the class is a name.
    pub of: Of,
}

/// What the entries are, as written: the spec before it is checked.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Entered {
    /// The modules and packages in the declaring file's own directory.
    #[serde(default)]
    pub beside: bool,
    /// The files matching a glob from the project root.
    #[serde(default)]
    pub glob: Option<Glob>,
    /// The entry names left out: stems, a package by its folder name.
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// What the names are compared with: the modules beside the declaring file, or the files of a glob; some left out.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "Entered")]
#[schemars(extend("oneOf" = [
    {"required": ["beside"], "properties": {"beside": {"const": true}}, "not": {"required": ["glob"]}},
    {"required": ["glob"], "properties": {"beside": {"const": false}, "glob": {"type": "string"}}},
]))]
pub struct Entries {
    /// The modules and packages in the declaring file's own directory.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub beside: bool,
    /// The files matching a glob from the project root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glob: Option<Glob>,
    /// The entry names left out: stems, a package by its folder name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,
}

/// What a `parity` rule judges: the names a file's classes declare against the entries they stand for.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ParitySpec {
    /// The classes of the file that declare names.
    pub names: Names,
    /// What the names are compared with.
    pub entries: Entries,
    /// How they must stand to one another.
    pub relation: Relation,
}

impl Called {
    /// The class name for a module in a directory and with a stem, as they are named on disk.
    #[must_use]
    pub fn of(&self, dir: &str, stem: &str) -> String {
        self.0.spelled(dir, stem)
    }

    /// The template as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl FromStr for Called {
    type Err = Invalid;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let template: Template = text.parse()?;
        template
            .spelled("Dir", "Stem")
            .contains(['*', '?', '[', ']', '{', '}', '\\'])
            .then(|| {
                Invalid::new(
                    Refusal::BadTemplate,
                    format!("`{text}`: a class name is exact, a glob character has no place in it"),
                )
            })
            .map_or(Ok(Self(template)), Err)
    }
}

impl TryFrom<String> for Called {
    type Error = Invalid;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<Called> for String {
    fn from(called: Called) -> Self {
        called.0.into()
    }
}

impl fmt::Display for Called {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl JsonSchema for Called {
    fn schema_name() -> Cow<'static, str> {
        "Called".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "minLength": 1,
            "description": "An exact class name; {Dir} and {Stem} stand for the module's directory and stem.",
        })
    }
}

impl Of {
    /// The name it is written by.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Attributes => "attributes",
            Self::Members => "members",
        }
    }
}

impl Of {
    /// What one name of it is called.
    #[must_use]
    pub fn singular(self) -> &'static str {
        match self {
            Self::Attributes => "attribute",
            Self::Members => "member",
        }
    }
}

impl fmt::Display for Of {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl Relation {
    /// The name it is written by.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Equal => "equal",
            Self::NamesInEntries => "names-in-entries",
            Self::EntriesInNames => "entries-in-names",
        }
    }

    /// Whether a name with no entry is a finding.
    #[must_use]
    pub fn names(self) -> bool {
        matches!(self, Self::Equal | Self::NamesInEntries)
    }

    /// Whether an entry with no name is a finding.
    #[must_use]
    pub fn entries(self) -> bool {
        matches!(self, Self::Equal | Self::EntriesInNames)
    }
}

impl fmt::Display for Relation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl TryFrom<Entered> for Entries {
    type Error = Invalid;

    fn try_from(entered: Entered) -> Result<Self, Self::Error> {
        match entered {
            Entered {
                beside: true,
                glob: None,
                exclude,
            } => Ok(Self {
                beside: true,
                glob: None,
                exclude,
            }),
            Entered {
                beside: false,
                glob: Some(glob),
                exclude,
            } => Ok(Self {
                beside: false,
                glob: Some(glob),
                exclude,
            }),
            Entered { .. } => Err(Invalid::new(
                Refusal::BadEntries,
                "entries set `beside: true` or a `glob`, one and not both",
            )),
        }
    }
}
