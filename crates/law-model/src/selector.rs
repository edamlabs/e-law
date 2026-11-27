use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;
use std::sync::LazyLock;

use regex::Regex;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

use crate::glob::Glob;
use crate::id::{RuleId, WORD};
use crate::invalid::{Invalid, Refusal};
use crate::rule::Rule;
use crate::tags::Tags;

const ALL: &str = "ALL";
const GLOB: [char; 4] = ['*', '?', '[', '{'];
const TAG: &str = "[^=]+=[^=]+";
const LITERAL: &str = "[a-z0-9.-]";
const META: &str = r"\*|\?|\[[a-z0-9]+\]|\{[a-z0-9.*?-]+(?:,[a-z0-9.*?-]+)*\}";

static LOWERCASE: LazyLock<Result<Regex, regex::Error>> =
    LazyLock::new(|| Regex::new(&format!("^(?:{})$", Selector::grammar())));

/// A choice of rules: `ALL`, an id, a lowercase id glob (`lexis.*`) or a tag membership (`law=5`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum Selector {
    /// Every rule.
    All,
    /// One rule by id.
    Id(RuleId),
    /// The rules whose id matches a glob.
    Glob(Glob),
    /// The rules whose tag `key` holds `value`.
    Tag {
        /// The tag's key.
        key: String,
        /// The value it must hold.
        value: String,
    },
}

impl Selector {
    /// Whether the rule is chosen.
    #[must_use]
    pub fn matches(&self, rule: &Rule) -> bool {
        self.chooses(&rule.id, &rule.tags)
    }

    /// Whether a rule of this id and these tags is chosen; a finding carries both.
    #[must_use]
    pub fn chooses(&self, id: &RuleId, tags: &Tags) -> bool {
        match self {
            Self::All => true,
            Self::Id(chosen) => chosen == id,
            Self::Glob(glob) => glob.matches(id.as_str()),
            Self::Tag { key, value } => tags.contains(key, value),
        }
    }

    fn grammar() -> String {
        format!("(?:{LITERAL}|{META})*(?:{META})(?:{LITERAL}|{META})*")
    }

    fn tag(text: &str, key: &str, value: &str) -> Result<Self, Invalid> {
        (!key.is_empty() && !value.is_empty() && !value.contains('='))
            .then(|| Self::Tag {
                key: key.to_owned(),
                value: value.to_owned(),
            })
            .ok_or_else(|| Invalid::new(Refusal::BadSelector, format!("`{text}` is not key=value")))
    }

    fn pattern(text: &str) -> Result<Self, Invalid> {
        let lowercase = LOWERCASE.as_ref().is_ok_and(|grammar| grammar.is_match(text));
        if lowercase {
            text.parse()
                .map(Self::Glob)
                .map_err(|invalid: Invalid| Invalid::new(Refusal::BadSelector, invalid.detail))
        } else {
            Err(Invalid::new(
                Refusal::BadSelector,
                format!("`{text}` is not an id glob of lowercase words, `*`, `?`, `[..]` and `{{..,..}}`"),
            ))
        }
    }

    fn id(text: &str) -> Result<Self, Invalid> {
        text.parse()
            .map(Self::Id)
            .map_err(|invalid: Invalid| Invalid::new(Refusal::BadSelector, invalid.detail))
    }
}

impl FromStr for Selector {
    type Err = Invalid;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match (text, text.split_once('=')) {
            (ALL, _) => Ok(Self::All),
            (_, Some((key, value))) => Self::tag(text, key, value),
            _ if text.contains(GLOB) => Self::pattern(text),
            _ => Self::id(text),
        }
    }
}

impl TryFrom<String> for Selector {
    type Error = Invalid;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<Selector> for String {
    fn from(selector: Selector) -> Self {
        selector.to_string()
    }
}

impl fmt::Display for Selector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::All => f.write_str(ALL),
            Self::Id(id) => write!(f, "{id}"),
            Self::Glob(glob) => write!(f, "{glob}"),
            Self::Tag { key, value } => write!(f, "{key}={value}"),
        }
    }
}

impl JsonSchema for Selector {
    fn schema_name() -> Cow<'static, str> {
        "Selector".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": format!("^(?:{ALL}|{TAG}|{WORD}\\.{WORD}|{})$", Self::grammar()),
            "description": "ALL, a rule id, a lowercase id glob (lexis.*) or a tag membership (law=5).",
        })
    }
}
