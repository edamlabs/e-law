use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

use crate::expression::Expression;
use crate::glob::Glob;
use crate::invalid::{Invalid, Refusal};

const DIR: &str = "{Dir}";
const STEM: &str = "{Stem}";
const PLACEHOLDERS: [&str; 2] = ["Dir", "Stem"];

/// A class-name glob where `{Dir}` and `{Stem}` stand for the module's directory and stem in `PascalCase`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Template(String);

/// The classes a module may define.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Classes {
    /// Exactly how many; absent, any number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
    /// How each is named.
    #[serde(default = "Template::any")]
    pub name: Template,
    /// The glob at least one base's bare name must match; absent, any bases.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bases: Option<Glob>,
}

/// The top-level functions a module may define.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Functions {
    /// At most how many; absent, any number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<u32>,
    /// Whether every one must be `async`.
    #[serde(default)]
    pub asynchronous: bool,
}

/// The top-level assignments a module may hold.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Constants {
    /// At most how many; absent, any number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<u32>,
    /// The expression each annotation must match, an assignment with none being the empty text; absent, any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotation: Option<Expression>,
    /// Whether a string literal may appear in their values.
    #[serde(default = "Constants::allowed")]
    pub literals: bool,
}

/// What the top level of every module in scope holds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShapeSpec {
    /// Its classes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classes: Option<Classes>,
    /// Its functions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub functions: Option<Functions>,
    /// Its constants.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constants: Option<Constants>,
}

impl Template {
    fn any() -> Self {
        Self("*".to_owned())
    }

    fn pascal(name: &str) -> String {
        name.split('_')
            .flat_map(|part| {
                let mut chars = part.chars();
                chars
                    .next()
                    .into_iter()
                    .flat_map(char::to_uppercase)
                    .chain(chars.flat_map(char::to_lowercase))
            })
            .collect()
    }

    fn unknown(text: &str) -> Option<&str> {
        text.split('{')
            .skip(1)
            .filter_map(|part| part.split_once('}').map(|(word, _)| word))
            .find(|word| {
                !word.is_empty()
                    && word.chars().all(|char| char.is_alphanumeric() || char == '_')
                    && !PLACEHOLDERS.contains(word)
            })
    }

    /// The template as text for a module in a directory and with a stem, as they are named on disk, both read as
    /// `PascalCase`.
    #[must_use]
    pub fn spelled(&self, dir: &str, stem: &str) -> String {
        self.0
            .replace(DIR, &Self::pascal(dir))
            .replace(STEM, &Self::pascal(stem))
    }

    /// The glob for a module in a directory and with a stem, both already in `PascalCase`.
    ///
    /// # Errors
    /// When the directory or the stem breaks the glob.
    pub fn fill(&self, dir: &str, stem: &str) -> Result<Glob, Invalid> {
        self.0.replace(DIR, dir).replace(STEM, stem).parse()
    }

    /// The glob for a module in a directory and with a stem, as they are named on disk: both read as `PascalCase`.
    ///
    /// # Errors
    /// When the directory or the stem breaks the glob.
    pub fn of(&self, dir: &str, stem: &str) -> Result<Glob, Invalid> {
        self.fill(&Self::pascal(dir), &Self::pascal(stem))
    }

    /// The template as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for Template {
    type Err = Invalid;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text.is_empty() {
            return Err(Invalid::new(Refusal::BadTemplate, "a template is not empty"));
        }
        if let Some(word) = Self::unknown(text) {
            return Err(Invalid::new(
                Refusal::BadTemplate,
                format!("`{text}`: unknown placeholder `{{{word}}}`, only {DIR} and {STEM}"),
            ));
        }
        Self(text.to_owned())
            .fill("Dir", "Stem")
            .map(|_| Self(text.to_owned()))
            .map_err(|invalid| Invalid::new(Refusal::BadTemplate, invalid.detail))
    }
}

impl TryFrom<String> for Template {
    type Error = Invalid;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<Template> for String {
    fn from(template: Template) -> Self {
        template.0
    }
}

impl fmt::Display for Template {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl JsonSchema for Template {
    fn schema_name() -> Cow<'static, str> {
        "Template".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "minLength": 1,
            "description": "A class-name glob; {Dir} and {Stem} stand for the module's directory and stem.",
        })
    }
}

impl Constants {
    fn allowed() -> bool {
        true
    }
}
