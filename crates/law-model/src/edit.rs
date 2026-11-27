use std::cmp::Ordering;
use std::ops::Range;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One change to the tree a fix makes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "edit", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Edit {
    /// A byte range of a file replaced by a text.
    Replace {
        /// The file.
        path: PathBuf,
        /// The bytes replaced.
        bytes: Range<usize>,
        /// What replaces them.
        text: String,
    },
    /// A file created with a text.
    Create {
        /// The new file.
        path: PathBuf,
        /// Its contents.
        text: String,
    },
    /// A file moved.
    Move {
        /// Where it was.
        from: PathBuf,
        /// Where it goes.
        to: PathBuf,
    },
}

impl Edit {
    fn key(&self) -> (u8, &Path, &Path, usize, usize, &str) {
        match self {
            Self::Replace { path, bytes, text } => (0, path, path, bytes.start, bytes.end, text),
            Self::Create { path, text } => (1, path, path, 0, 0, text),
            Self::Move { from, to } => (2, from, to, 0, 0, ""),
        }
    }
}

impl PartialOrd for Edit {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Edit {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key().cmp(&other.key())
    }
}
