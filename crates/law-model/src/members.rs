use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::invalid::{Invalid, Refusal};

/// The order a class's methods must follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Order {
    /// No private method after a public one.
    PrivatesFirst,
}

/// The lineage a private method must have: `_<stem>_<what>`, its stem a public of the class or a common word.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Lineage {
    /// The stems a private may take with no public of that name: `common` admits `_common_<what>`.
    #[serde(default)]
    pub common: Vec<String>,
}

/// What a class's methods must hold, as written: the spec before it is checked.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Members {
    /// The lineage of every private method.
    #[serde(default)]
    pub lineage: Option<Lineage>,
    /// The order of the methods.
    #[serde(default)]
    pub order: Option<Order>,
    /// The decorators that hand a method to a framework, which calls it: a method they decorate is judged by neither.
    #[serde(default)]
    pub hooks: Vec<String>,
}

/// What a class's methods must hold: their lineage, their order, or both.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "Members")]
#[schemars(extend("anyOf" = [
    {"required": ["lineage"], "properties": {"lineage": {"type": "object"}}},
    {"required": ["order"], "properties": {"order": {"type": "string"}}},
]))]
pub struct MembersSpec {
    /// The lineage of every private method.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lineage: Option<Lineage>,
    /// The order of the methods.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<Order>,
    /// The decorators that hand a method to a framework, which calls it: a method they decorate is judged by neither.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<String>,
}

impl TryFrom<Members> for MembersSpec {
    type Error = Invalid;

    fn try_from(members: Members) -> Result<Self, Self::Error> {
        match members {
            Members {
                lineage: None,
                order: None,
                ..
            } => Err(Invalid::new(
                Refusal::EmptyMembers,
                "a members check sets `lineage`, `order` or both",
            )),
            Members { lineage, order, hooks } => Ok(Self { lineage, order, hooks }),
        }
    }
}
