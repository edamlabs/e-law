use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::case::Cases;
use crate::check::Check;
use crate::fix::FixPolicy;
use crate::id::{Former, RuleId};
use crate::message::Message;
use crate::scope::Scope;
use crate::severity::Severity;
use crate::tags::Tags;

/// One atomic check: what it says, where it looks, what runs it, how it is fixed and the cases that prove it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// Its identity.
    pub id: RuleId,
    /// The ids it went by before: a pragma naming one is respelled with this rule's id.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub formerly: Vec<Former>,
    /// What it says when it fires.
    pub message: Message,
    /// Its labels.
    #[serde(default)]
    pub tags: Tags,
    /// Where it looks.
    #[serde(default)]
    pub scope: Scope,
    /// How hard it blocks.
    #[serde(default)]
    pub severity: Severity,
    /// The engine that runs it, and its spec.
    pub check: Check,
    /// Its fix, or why it has none.
    pub fix: FixPolicy,
    /// The snippets that prove it.
    #[serde(default)]
    pub cases: Cases,
}
