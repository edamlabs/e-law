use schemars::JsonSchema;
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};

use crate::edit::Edit;

const REASON: &str = "reason";

/// Whether a fix may be applied without review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Safety {
    /// Applied by `--fix`.
    Safe,
    /// Applied only with `--unsafe-fixes`.
    Unsafe,
}

/// What a rule promises about fixing what it finds; every rule states it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, JsonSchema)]
#[serde(tag = "safety", rename_all = "kebab-case", deny_unknown_fields)]
pub enum FixPolicy {
    /// A fix applied by `--fix`.
    Safe,
    /// A fix applied only with `--unsafe-fixes`.
    Unsafe,
    /// No fix, and why.
    None {
        /// Why the rule has no fix.
        reason: String,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Promise {
    Safe,
    Unsafe,
    None,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FixDocument {
    safety: Promise,
    #[serde(default)]
    reason: Option<String>,
}

impl<'de> Deserialize<'de> for FixPolicy {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let FixDocument { safety, reason } = FixDocument::deserialize(deserializer)?;
        match (safety, reason) {
            (Promise::Safe, None) => Ok(Self::Safe),
            (Promise::Unsafe, None) => Ok(Self::Unsafe),
            (Promise::None, Some(reason)) => Ok(Self::None { reason }),
            (Promise::None, None) => Err(de::Error::missing_field(REASON)),
            (_, Some(_)) => Err(de::Error::unknown_field(REASON, &["safety"])),
        }
    }
}

/// The edits that resolve one finding, and their safety.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Fix {
    /// Whether the edits may be applied without review.
    pub safety: Safety,
    /// The edits, applied together or not at all.
    pub edits: Vec<Edit>,
}
