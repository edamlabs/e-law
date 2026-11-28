use law_model::{FixPolicy, Position, Safety};
use serde::Deserialize;

/// A place in ruff's JSON: 1-based row, 1-based column in characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct RuffLocation {
    /// The 1-based row.
    pub row: u32,
    /// The 1-based column, in characters.
    pub column: u32,
}

/// Whether ruff would apply a fix, as its JSON writes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Applicability {
    /// Shown, never applied.
    DisplayOnly,
    /// Applied with `--unsafe-fixes`.
    Unsafe,
    /// Applied with `--fix`.
    Safe,
}

/// One edit of a ruff fix: the text between two places replaced.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RuffEdit {
    /// The replacement.
    pub content: String,
    /// Where it starts.
    pub location: RuffLocation,
    /// Where it ends.
    pub end_location: RuffLocation,
}

/// A ruff fix: its applicability and its edits.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RuffFix {
    /// Whether ruff would apply it.
    pub applicability: Applicability,
    /// Its edits.
    pub edits: Vec<RuffEdit>,
}

/// One diagnostic of `ruff check --output-format json`, the fields e-law reads.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RuffDiagnostic {
    /// The code, or a name such as `invalid-syntax` for what is no rule's.
    pub code: String,
    /// The file, absolute.
    pub filename: String,
    /// Where it starts.
    pub location: RuffLocation,
    /// Where it ends.
    pub end_location: RuffLocation,
    /// What ruff says.
    pub message: String,
    /// Its fix, when ruff has one.
    pub fix: Option<RuffFix>,
}

impl RuffLocation {
    /// The same place as e-law writes it.
    #[must_use]
    pub fn position(self) -> Position {
        Position {
            line: self.row,
            column: self.column,
        }
    }
}

impl Applicability {
    /// The safety of a fix ruff marks so, under a rule's policy: the weaker of the two; none when the rule promises no
    /// fix or ruff would only show it.
    #[must_use]
    pub fn safety(self, policy: &FixPolicy) -> Option<Safety> {
        match (self, policy) {
            (Self::DisplayOnly, _) | (_, FixPolicy::None { .. }) => None,
            (Self::Safe, FixPolicy::Safe) => Some(Safety::Safe),
            (Self::Safe, FixPolicy::Unsafe) | (Self::Unsafe, FixPolicy::Safe | FixPolicy::Unsafe) => {
                Some(Safety::Unsafe)
            }
        }
    }
}
