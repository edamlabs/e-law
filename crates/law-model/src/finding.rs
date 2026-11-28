use std::cmp::Ordering;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::fix::Fix;
use crate::id::RuleId;
use crate::severity::Severity;
use crate::span::Span;
use crate::tags::Tags;

/// One place a rule fired; ordered by path, start byte, then rule.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    /// The rule that fired.
    pub rule: RuleId,
    /// The file, from the project root.
    pub path: PathBuf,
    /// Where in the file.
    pub span: Span,
    /// What the rule says there.
    pub message: String,
    /// How hard it blocks.
    pub severity: Severity,
    /// The rule's labels.
    pub tags: Tags,
    /// The edits that resolve it, when the rule has a fix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix: Option<Fix>,
}

impl PartialOrd for Finding {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Finding {
    fn cmp(&self, other: &Self) -> Ordering {
        (&self.path, self.span.bytes.start, &self.rule)
            .cmp(&(&other.path, other.span.bytes.start, &other.rule))
            .then_with(|| {
                (&self.span, &self.message, self.severity, &self.tags, &self.fix).cmp(&(
                    &other.span,
                    &other.message,
                    other.severity,
                    &other.tags,
                    &other.fix,
                ))
            })
    }
}
