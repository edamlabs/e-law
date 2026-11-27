use std::cmp::Ordering;
use std::ops::Range;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A place in a file: 1-based line, 1-based column in characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Position {
    /// The 1-based line.
    pub line: u32,
    /// The 1-based column, in characters.
    pub column: u32,
}

/// A stretch of a file, as positions and as a byte range.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Span {
    /// Where it starts.
    pub start: Position,
    /// Where it ends.
    pub end: Position,
    /// The same stretch in bytes.
    pub bytes: Range<usize>,
}

impl PartialOrd for Span {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Span {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.bytes.start, self.bytes.end, self.start, self.end).cmp(&(
            other.bytes.start,
            other.bytes.end,
            other.start,
            other.end,
        ))
    }
}
