use std::fmt;
use std::path::{Path, PathBuf};

use law_model::{Rooted, RuleId};

use crate::tool::Tool;

/// What an external tool could not do for a run.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Failure {
    /// The tool is nowhere it is looked for, and selected rules need it.
    #[error("not found beside law, in .venv/bin nor on PATH; needed by {}", Ids(rules))]
    Missing {
        /// The selected rules that need it.
        rules: Vec<RuleId>,
    },
    /// The tool did not start.
    #[error("{} did not start: {message}", path.display())]
    Spawn {
        /// The executable.
        path: PathBuf,
        /// What the system said.
        message: String,
    },
    /// The tool exited with other than success.
    #[error("exited with {}: {stderr}", code.map_or_else(|| "a signal".to_owned(), |code| code.to_string()))]
    Exit {
        /// Its exit code; none when a signal ended it.
        code: Option<i32>,
        /// Its stderr's first lines.
        stderr: String,
    },
    /// The tool's output is not the document expected.
    #[error("its output is not the document expected: {message}")]
    Document {
        /// What the decoder said.
        message: String,
    },
    /// A diagnostic about a file outside the project root.
    #[error("{}: a diagnostic outside the project root", path.display())]
    Outside {
        /// The file, as the tool wrote it.
        path: PathBuf,
    },
    /// A diagnostic at a place its file does not have.
    #[error("{}:{line}:{column}: a place the file does not have", path.display())]
    Place {
        /// The file.
        path: PathBuf,
        /// The 1-based line.
        line: u32,
        /// The 1-based column.
        column: u32,
    },
    /// A diagnostic no catalogue rule asked for: a file the tool could not read, as its syntax error.
    #[error("{}:{line}:{column}: {code}: {message}", path.display())]
    Unasked {
        /// The file.
        path: PathBuf,
        /// The 1-based line.
        line: u32,
        /// The 1-based column.
        column: u32,
        /// The tool's code for it.
        code: String,
        /// What the tool said.
        message: String,
    },
    /// A diagnostic placed nowhere.
    #[error("`{rule}` placed nowhere: {message}")]
    Placeless {
        /// The tool's rule.
        rule: String,
        /// What the tool said.
        message: String,
    },
    /// A kind of diagnostic e-law does not know: the tool grew.
    #[error("`{kind}`: a kind of diagnostic e-law does not know")]
    Unknown {
        /// The kind, or the line, as the tool wrote it.
        kind: String,
    },
    /// The tool refused the project: its own error, or a configuration it reports broken.
    #[error("{message}")]
    Refused {
        /// What the tool said.
        message: String,
    },
    /// A file that could not be read or written.
    #[error("{}: {message}", path.display())]
    Io {
        /// The file.
        path: PathBuf,
        /// What the system said.
        message: String,
    },
}

/// What a tool could not do, and which tool.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{tool}: {failure}")]
pub struct Fault {
    /// The tool.
    pub tool: Tool,
    /// What it could not do.
    pub failure: Failure,
}

struct Ids<'a>(&'a [RuleId]);

impl fmt::Display for Ids<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ids: Vec<String> = self.0.iter().map(|id| format!("`{id}`")).collect();
        f.write_str(&ids.join(", "))
    }
}

impl Failure {
    /// A failed read or write of a file.
    #[must_use]
    pub fn io(path: &Path, error: &std::io::Error) -> Self {
        Self::Io {
            path: path.to_owned(),
            message: error.to_string(),
        }
    }
}

impl From<Fault> for law_engine::Fault {
    fn from(fault: Fault) -> Self {
        Self::External {
            engine: fault.tool.name().to_owned(),
            message: fault.failure.to_string(),
        }
    }
}

impl Rooted for Fault {
    fn rooted(self, root: &Path) -> Self {
        let failure = match self.failure {
            Failure::Spawn { path, message } => Failure::Spawn {
                path: path.rooted(root),
                message,
            },
            Failure::Io { path, message } => Failure::Io {
                path: path.rooted(root),
                message,
            },
            other => other,
        };
        Self {
            tool: self.tool,
            failure,
        }
    }
}
