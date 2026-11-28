use std::borrow::Cow;
use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

use regex::Regex;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

use crate::invalid::{Invalid, Refusal};

/// A regular expression compiled at load, matched against the whole text or found within it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Expression {
    text: String,
    regex: Regex,
    part: Regex,
}

impl Expression {
    /// The expression as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Whether the whole text matches.
    #[must_use]
    pub fn matches(&self, text: &str) -> bool {
        self.regex.is_match(text)
    }

    /// Whether it matches somewhere in the text.
    #[must_use]
    pub fn finds(&self, text: &str) -> bool {
        self.part.is_match(text)
    }
}

impl FromStr for Expression {
    type Err = Invalid;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Regex::new(&format!("^(?:{text})$"))
            .and_then(|regex| Regex::new(text).map(|part| (regex, part)))
            .map(|(regex, part)| Self {
                text: text.to_owned(),
                regex,
                part,
            })
            .map_err(|error| Invalid::new(Refusal::BadExpression, format!("`{text}`: {error}")))
    }
}

impl TryFrom<String> for Expression {
    type Error = Invalid;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<Expression> for String {
    fn from(expression: Expression) -> Self {
        expression.text
    }
}

impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl PartialEq for Expression {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
    }
}

impl Eq for Expression {}

impl PartialOrd for Expression {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Expression {
    fn cmp(&self, other: &Self) -> Ordering {
        self.text.cmp(&other.text)
    }
}

impl Hash for Expression {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.text.hash(state);
    }
}

impl JsonSchema for Expression {
    fn schema_name() -> Cow<'static, str> {
        "Expression".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "format": "regex",
            "description": "A regular expression matched against the whole text.",
        })
    }
}
