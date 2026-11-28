use std::borrow::Cow;
use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::str::FromStr;

use globset::{GlobBuilder, GlobMatcher};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

use crate::invalid::{Invalid, Refusal};

/// A glob compiled at load; `*` never crosses a `/`, `**` does.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Glob {
    text: String,
    matcher: GlobMatcher,
}

impl Glob {
    /// The glob as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Whether a path, relative to the project root, matches.
    #[must_use]
    pub fn matches(&self, path: impl AsRef<Path>) -> bool {
        self.matcher.is_match(path)
    }
}

impl FromStr for Glob {
    type Err = Invalid;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        GlobBuilder::new(text)
            .literal_separator(true)
            .build()
            .map(|glob| Self {
                text: text.to_owned(),
                matcher: glob.compile_matcher(),
            })
            .map_err(|error| Invalid::new(Refusal::BadGlob, format!("`{text}`: {}", error.kind())))
    }
}

impl TryFrom<String> for Glob {
    type Error = Invalid;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<Glob> for String {
    fn from(glob: Glob) -> Self {
        glob.text
    }
}

impl fmt::Display for Glob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl PartialEq for Glob {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
    }
}

impl Eq for Glob {}

impl PartialOrd for Glob {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Glob {
    fn cmp(&self, other: &Self) -> Ordering {
        self.text.cmp(&other.text)
    }
}

impl Hash for Glob {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.text.hash(state);
    }
}

impl JsonSchema for Glob {
    fn schema_name() -> Cow<'static, str> {
        "Glob".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "minLength": 1,
            "description": "A glob; `*` never crosses a `/`, `**` does.",
        })
    }
}
