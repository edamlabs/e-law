use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use law_model::{Invalid, Rooted, RuleId, Selector};

/// A part of a rule or an exception the engine does not check yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Feature {
    /// An engine this build does not run.
    Engine,
    /// A `files` case, which only the `graph` engine proves.
    Files,
}

/// Why a run could not be planned, or a path could not be checked; every one is collected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Fault {
    /// A `pattern` rule ast-grep does not compile, or whose node kinds it cannot determine.
    #[error("rule `{rule}`: the pattern does not compile: {message}")]
    BadPattern {
        /// The rule.
        rule: RuleId,
        /// What ast-grep said.
        message: String,
    },
    /// A `pattern` rule whose spec's `fix` and fix policy disagree.
    #[error("rule `{rule}`: a fix in the spec needs a `safe` or `unsafe` policy, and such a policy needs a fix")]
    FixMismatch {
        /// The rule.
        rule: RuleId,
    },
    /// A scope glob the engine could not compile into a set.
    #[error("rule `{rule}`: the scope does not compile: {message}")]
    BadScope {
        /// The rule.
        rule: RuleId,
        /// What the glob compiler said.
        message: String,
    },
    /// A class-name template that does not compile once filled for a module.
    #[error("{}: rule `{rule}`: the class-name template does not compile here: {message}", path.display())]
    BadTemplate {
        /// The rule.
        rule: RuleId,
        /// The module.
        path: PathBuf,
        /// What the glob compiler said.
        message: String,
    },
    /// A rule asking for what this cut does not check, refused so nothing is silently unchecked.
    #[error("rule `{rule}`: {feature} is not implemented")]
    Unsupported {
        /// The rule.
        rule: RuleId,
        /// What it asks for.
        feature: Feature,
    },
    /// An external rule no batch engine handed to the run checks.
    #[error("rule `{rule}`: no engine of the run checks it")]
    NoEngine {
        /// The rule.
        rule: RuleId,
    },
    /// What an external engine could not do: run its tool, or read what the tool said.
    #[error("{engine}: {message}")]
    External {
        /// The engine.
        engine: String,
        /// What went wrong, the tool's own words included.
        message: String,
    },
    /// An exception naming a symbol, which this cut does not resolve.
    #[error("exception to `{rule}`: `symbol` is not implemented")]
    SymbolException {
        /// The exception's selector.
        rule: Selector,
    },
    /// A catalogue that could not be written as canonical JSON to be digested.
    #[error("catalogue `{catalogue}`: no digest: {message}")]
    Digest {
        /// The catalogue's name.
        catalogue: String,
        /// What the encoder said.
        message: String,
    },
    /// The embedded bank of the interpreter's classes, which did not decode.
    #[error("the bank of builtin classes does not decode: {message}")]
    Bank {
        /// What the decoder said.
        message: String,
    },
    /// A reserved rule whose own id the model refuses.
    #[error("a reserved rule: {0}")]
    Reserved(Invalid),
    /// A path to check outside the project root.
    #[error("{}: outside the project root", path.display())]
    Outside {
        /// The path.
        path: PathBuf,
    },
    /// A module a fix would move, which cannot be moved with every import of it rewritten exactly.
    #[error("{}: not moved: {why}", path.display())]
    Unmovable {
        /// The module.
        path: PathBuf,
        /// Why not.
        why: String,
    },
    /// A fix that could not be applied.
    #[error(transparent)]
    Fix(law_fix::Fault),
    /// A path that could not be read, walked or decoded as UTF-8.
    #[error("{}: {message}", path.display())]
    Io {
        /// The path.
        path: PathBuf,
        /// What went wrong.
        message: String,
    },
}

impl fmt::Display for Feature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Engine => "its engine",
            Self::Files => "a `files` case",
        })
    }
}

impl Fault {
    /// A failed read of a path.
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
            Self::BadTemplate { rule, path, message } => Self::BadTemplate {
                rule,
                path: path.rooted(root),
                message,
            },
            Self::Outside { path } => Self::Outside {
                path: path.rooted(root),
            },
            Self::Io { path, message } => Self::Io {
                path: path.rooted(root),
                message,
            },
            Self::Unmovable { path, why } => Self::Unmovable {
                path: path.rooted(root),
                why,
            },
            Self::Fix(fault) => Self::Fix(fault.rooted(root)),
            other => other,
        }
    }
}
