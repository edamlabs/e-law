use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

use crate::glob::Glob;
use crate::id::RuleId;
use crate::invalid::{Invalid, Refusal};

/// The name of an entity kind: one lowercase word.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Kind(String);

/// An entity kind: where its modules live, how its entity classes are named, what each holds, what it descends from.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// The kind.
    pub kind: Kind,
    /// The paths its modules live at, from the project root.
    pub home: Vec<Glob>,
    /// The ending of an entity class's name; absent, every top-level class of a module in its home is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    /// The methods each entity class has, its own or inherited.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<String>,
    /// The names each entity class binds in its body, its own or inherited.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<String>,
    /// The dotted names one of which an entity class descends from.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub line: Vec<String>,
    /// The dotted names one of which an entity class writes among its own direct bases.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bases: Vec<String>,
    /// The dotted names none of which an entity class descends from.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub never: Vec<String>,
    /// The numbers a metric reads, by name; a project overrides only these.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub thresholds: BTreeMap<String, u32>,
}

/// What a `profile` rule judges of the entity classes of its kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Aspect {
    /// A module in a home declares at least one entity class.
    Present,
    /// Every top-level class of a module in a home ends with the suffix.
    Surname,
    /// Each entity class has every method of the profile.
    Methods,
    /// Each entity class binds every attribute of the profile.
    Attributes,
    /// Each entity class descends from one name of the profile's `line`.
    Line,
    /// Each entity class writes one name of the profile's `bases` among its own direct bases.
    Bases,
    /// No entity class descends from a name of the profile's `never`.
    Never,
}

/// A `profile` check as written: the spec before it is checked.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Profiled {
    /// The kinds judged.
    #[schemars(length(min = 1))]
    pub kinds: Vec<Kind>,
    /// What is judged.
    pub aspect: Aspect,
}

/// What a `profile` rule judges: one aspect of the entity classes of one or more kinds.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "Profiled")]
pub struct ProfileSpec {
    /// The kinds judged.
    #[schemars(length(min = 1))]
    pub kinds: Vec<Kind>,
    /// What is judged.
    pub aspect: Aspect,
}

impl Aspect {
    /// The name it is written by.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Present => "present",
            Self::Surname => "surname",
            Self::Methods => "methods",
            Self::Attributes => "attributes",
            Self::Line => "line",
            Self::Bases => "bases",
            Self::Never => "never",
        }
    }
}

impl fmt::Display for Aspect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl Profile {
    /// Whether it gives the data an aspect judges: a suffix for `present` and `surname`, a non-empty list otherwise.
    #[must_use]
    pub fn feeds(&self, aspect: Aspect) -> bool {
        match aspect {
            Aspect::Present | Aspect::Surname => self.suffix.is_some(),
            Aspect::Methods => !self.methods.is_empty(),
            Aspect::Attributes => !self.attributes.is_empty(),
            Aspect::Line => !self.line.is_empty(),
            Aspect::Bases => !self.bases.is_empty(),
            Aspect::Never => !self.never.is_empty(),
        }
    }
}

impl Profile {
    /// The profile with the thresholds a project overrides in place of its own; a name it does not declare is left.
    #[must_use]
    pub fn tuned(mut self, overrides: &BTreeMap<String, u32>) -> Self {
        self.thresholds
            .iter_mut()
            .filter_map(|(name, number)| overrides.get(name).map(|over| (number, over)))
            .for_each(|(number, over)| *number = *over);
        self
    }
}

impl TryFrom<Profiled> for ProfileSpec {
    type Error = Invalid;

    fn try_from(profiled: Profiled) -> Result<Self, Self::Error> {
        match profiled {
            Profiled { kinds, .. } if kinds.is_empty() => Err(Invalid::new(
                Refusal::EmptyKinds,
                "a profile check names at least one kind",
            )),
            Profiled { kinds, aspect } => Ok(Self { kinds, aspect }),
        }
    }
}

impl Kind {
    /// The kind as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for Kind {
    type Err = Invalid;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        RuleId::word(text)
            .then(|| Self(text.to_owned()))
            .ok_or_else(|| Invalid::new(Refusal::BadKind, format!("`{text}` is not one lowercase word")))
    }
}

impl TryFrom<String> for Kind {
    type Error = Invalid;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<Kind> for String {
    fn from(kind: Kind) -> Self {
        kind.0
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl JsonSchema for Kind {
    fn schema_name() -> Cow<'static, str> {
        "Kind".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": "^[a-z][a-z0-9-]*$",
            "description": "An entity kind: one lowercase word.",
        })
    }
}
