use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

use crate::invalid::{Invalid, Refusal};

pub(crate) const WORD: &str = "[a-z][a-z0-9-]*";

/// A rule's identity: `<namespace>.<name>`, each part one lowercase word.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RuleId {
    text: String,
    dot: usize,
}

/// What a rule was called before its id was `<namespace>.<name>`: one lowercase word.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Former(String);

impl RuleId {
    /// The part before the dot.
    #[must_use]
    pub fn namespace(&self) -> &str {
        &self.text[..self.dot]
    }

    /// The part after the dot.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.text[self.dot + 1..]
    }

    /// The id as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Whether a text is one lowercase word: `[a-z][a-z0-9-]*`.
    #[must_use]
    pub fn word(text: &str) -> bool {
        let mut chars = text.chars();
        chars.next().is_some_and(|first| first.is_ascii_lowercase())
            && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    }
}

impl FromStr for RuleId {
    type Err = Invalid;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        text.split_once('.')
            .filter(|(namespace, name)| Self::word(namespace) && Self::word(name))
            .map(|(namespace, _)| Self {
                text: text.to_owned(),
                dot: namespace.len(),
            })
            .ok_or_else(|| {
                Invalid::new(
                    Refusal::BadId,
                    format!("`{text}` is not <namespace>.<name>, each {WORD}"),
                )
            })
    }
}

impl TryFrom<String> for RuleId {
    type Error = Invalid;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<RuleId> for String {
    fn from(id: RuleId) -> Self {
        id.text
    }
}

impl fmt::Display for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl JsonSchema for RuleId {
    fn schema_name() -> Cow<'static, str> {
        "RuleId".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": format!("^{WORD}\\.{WORD}$"),
            "description": "A rule's identity: <namespace>.<name>.",
        })
    }
}

impl Former {
    /// The id as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for Former {
    type Err = Invalid;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        RuleId::word(text)
            .then(|| Self(text.to_owned()))
            .ok_or_else(|| Invalid::new(Refusal::BadId, format!("`{text}` is not one lowercase word, {WORD}")))
    }
}

impl TryFrom<String> for Former {
    type Error = Invalid;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<Former> for String {
    fn from(former: Former) -> Self {
        former.0
    }
}

impl fmt::Display for Former {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl JsonSchema for Former {
    fn schema_name() -> Cow<'static, str> {
        "Former".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": format!("^{WORD}$"),
            "description": "What a rule was called before: one lowercase word.",
        })
    }
}
