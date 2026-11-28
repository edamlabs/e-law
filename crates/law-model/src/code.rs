use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

use crate::invalid::{Invalid, Refusal};

const SHAPE: &str = "[A-Z]+[0-9]+";

/// A code an external linter names its rules by: capitals, then digits (`T201`, `TID251`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Code(String);

impl Code {
    /// The code as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether a text is a code: one or more capitals, then one or more digits, nothing else.
    #[must_use]
    pub fn shaped(text: &str) -> bool {
        let digits = text.trim_start_matches(|c: char| c.is_ascii_uppercase());
        digits.len() < text.len() && !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
    }
}

impl FromStr for Code {
    type Err = Invalid;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::shaped(text)
            .then(|| Self(text.to_owned()))
            .ok_or_else(|| Invalid::new(Refusal::BadCode, format!("`{text}` is not a code, {SHAPE}")))
    }
}

impl TryFrom<String> for Code {
    type Error = Invalid;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<Code> for String {
    fn from(code: Code) -> Self {
        code.0
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl JsonSchema for Code {
    fn schema_name() -> Cow<'static, str> {
        "Code".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": format!("^{SHAPE}$"),
            "description": "A code an external linter names its rules by.",
        })
    }
}
