use std::io;
use std::ops::Range;
use std::path::{Path, PathBuf};

use law_model::{Rooted, RuleId};

/// Why a fix could not be applied; the file it names is left as it was.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Fault {
    /// A file that could not be read or written.
    #[error("{}: {message}", path.display())]
    Io {
        /// The file.
        path: PathBuf,
        /// What the system said.
        message: String,
    },
    /// An edit whose bytes fall outside the file, or split a character.
    #[error("{}: the edit of bytes {}..{} does not fit the file", path.display(), bytes.start, bytes.end)]
    Range {
        /// The file.
        path: PathBuf,
        /// The bytes the edit replaces.
        bytes: Range<usize>,
    },
    /// A move onto a path where a file already is; nothing of the fix is written.
    #[error("{}: a move onto a file that exists", path.display())]
    Occupied {
        /// The path the move lands on.
        path: PathBuf,
    },
    /// A fix that creates a file, which this cut does not apply.
    #[error("rule `{rule}`: a fix that creates a file is not implemented")]
    Unsupported {
        /// The rule whose fix it is.
        rule: RuleId,
    },
}

impl Fault {
    /// A failed read or write of a file.
    #[must_use]
    pub fn io(path: &Path, error: &io::Error) -> Self {
        Self::Io {
            path: path.to_owned(),
            message: error.to_string(),
        }
    }
}

impl Rooted for Fault {
    fn rooted(self, root: &Path) -> Self {
        match self {
            Self::Io { path, message } => Self::Io {
                path: path.rooted(root),
                message,
            },
            Self::Range { path, bytes } => Self::Range {
                path: path.rooted(root),
                bytes,
            },
            Self::Occupied { path } => Self::Occupied {
                path: path.rooted(root),
            },
            unsupported @ Self::Unsupported { .. } => unsupported,
        }
    }
}
