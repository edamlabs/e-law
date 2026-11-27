use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::de::value::MapAccessDeserializer;
use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use crate::invalid::{Invalid, Refusal};

const BAD_CASE: &str = "a case sets `code`, with an optional `path`, or a non-empty `files`, with no `path`";

/// What a case gives: one snippet, or a set of files, each by its path from the project root.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Snippet {
    /// One snippet.
    Code(String),
    /// Files by their path from the project root: a project of its own.
    Files(BTreeMap<PathBuf, String>),
}

/// A snippet a rule is proven against, with no path at its scope's sample path; or a set of files.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "CaseDocument")]
pub struct Case {
    /// The path the snippet pretends to sit at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    /// The snippet, or the files.
    #[serde(flatten)]
    pub snippet: Snippet,
}

/// A snippet a fix is proven against: after the fix, `code` becomes `expect` and the check is silent.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FixedCase {
    /// The path the snippet pretends to sit at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    /// The snippet before the fix.
    pub code: String,
    /// The snippet after the fix.
    pub expect: String,
}

/// The snippets a rule must pass, must catch, and must fix.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Cases {
    /// Snippets the rule must pass.
    #[serde(default)]
    pub valid: Vec<Case>,
    /// Snippets the rule must catch.
    #[serde(default)]
    pub invalid: Vec<Case>,
    /// Snippets the rule's fix must resolve.
    #[serde(default)]
    pub fixed: Vec<FixedCase>,
}

#[derive(JsonSchema)]
#[serde(untagged)]
enum CaseDocument {
    Code(String),
    Placed(PlacedCase),
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("anyOf" = [
    {"required": ["code"], "properties": {"code": {"type": "string"}, "files": {"type": "null"}}},
    {"required": ["files"], "properties": {
        "files": {"type": "object", "minProperties": 1},
        "code": {"type": "null"},
        "path": {"type": "null"},
    }},
]))]
struct PlacedCase {
    #[serde(default)]
    path: Option<PathBuf>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    files: Option<BTreeMap<PathBuf, String>>,
}

struct CaseVisitor;

impl<'de> Visitor<'de> for CaseVisitor {
    type Value = CaseDocument;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a snippet, a table of `code` and an optional `path`, or a table of `files`")
    }

    fn visit_str<E: de::Error>(self, code: &str) -> Result<CaseDocument, E> {
        Ok(CaseDocument::Code(code.to_owned()))
    }

    fn visit_string<E: de::Error>(self, code: String) -> Result<CaseDocument, E> {
        Ok(CaseDocument::Code(code))
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<CaseDocument, A::Error> {
        PlacedCase::deserialize(MapAccessDeserializer::new(map)).map(CaseDocument::Placed)
    }
}

impl<'de> Deserialize<'de> for CaseDocument {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(CaseVisitor)
    }
}

impl TryFrom<CaseDocument> for Case {
    type Error = Invalid;

    fn try_from(document: CaseDocument) -> Result<Self, Self::Error> {
        match document {
            CaseDocument::Code(code) => Ok(Self {
                path: None,
                snippet: Snippet::Code(code),
            }),
            CaseDocument::Placed(PlacedCase {
                path,
                code: Some(code),
                files: None,
            }) => Ok(Self {
                path,
                snippet: Snippet::Code(code),
            }),
            CaseDocument::Placed(PlacedCase {
                path: None,
                code: None,
                files: Some(files),
            }) if !files.is_empty() => Ok(Self {
                path: None,
                snippet: Snippet::Files(files),
            }),
            CaseDocument::Placed(_) => Err(Invalid::new(Refusal::BadCase, BAD_CASE)),
        }
    }
}
