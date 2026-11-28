use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use law_model::{Aspect, Former, Invalid, Kind, Measure, Rooted, RuleId};

/// What a profile gives no data for, when a `metric` rule needs it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Lacking {
    /// The suffix that tells an entity class from a companion.
    #[error("suffix")]
    Suffix,
    /// A threshold, by its name.
    #[error("threshold `{0}`")]
    Threshold(String),
}

/// Why a configuration or a catalogue could not be loaded; every one is collected, never the first alone.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Fault {
    /// A rule id declared twice.
    #[error("{}: rule `{id}` is already declared in {}", second.display(), first.display())]
    DuplicateRule {
        /// The rule id.
        id: RuleId,
        /// Where it was declared first.
        first: PathBuf,
        /// Where it was declared again.
        second: PathBuf,
    },
    /// A former id claimed by two rules.
    #[error("{}: former id `{id}` of `{second}` is already `{first}`'s", path.display())]
    DuplicateFormer {
        /// The former id.
        id: Former,
        /// The rule that claimed it first.
        first: RuleId,
        /// The rule that claimed it again.
        second: RuleId,
        /// Where the second is declared.
        path: PathBuf,
    },
    /// A profile kind declared twice.
    #[error("{}: profile `{kind}` is already declared in {}", second.display(), first.display())]
    DuplicateProfile {
        /// The kind.
        kind: Kind,
        /// Where it was declared first.
        first: PathBuf,
        /// Where it was declared again.
        second: PathBuf,
    },
    /// Two rules taking the same external diagnostics, with nothing to tell them apart.
    #[error("{}: rules `{first}` and `{second}` both take every {key} diagnostic: nothing tells them apart", path.display())]
    Indistinct {
        /// What both take: the tool and its code or rule name.
        key: String,
        /// The rule declared first.
        first: RuleId,
        /// The rule declared again.
        second: RuleId,
        /// Where the second is declared.
        path: PathBuf,
    },
    /// A kind no loaded profile declares.
    #[error("{}: no profile declares kind `{kind}`", path.display())]
    UnknownKind {
        /// Where the kind is named.
        path: PathBuf,
        /// The kind.
        kind: Kind,
    },
    /// A `profile` rule judging an aspect one of its kinds' profiles gives no data for.
    #[error("{}: rule `{rule}` judges `{aspect}`, which profile `{kind}` gives no data for", path.display())]
    UnfedAspect {
        /// Where the rule is declared.
        path: PathBuf,
        /// The rule.
        rule: RuleId,
        /// The kind whose profile is silent.
        kind: Kind,
        /// The aspect.
        aspect: Aspect,
    },
    /// A `metric` rule measuring what its kind's profile gives no data for.
    #[error("{}: rule `{rule}` measures `{measure}`, which profile `{kind}` gives no {lacking} for", path.display())]
    UnfedMeasure {
        /// Where the rule is declared.
        path: PathBuf,
        /// The rule.
        rule: RuleId,
        /// The kind whose profile is silent.
        kind: Kind,
        /// The measure.
        measure: Measure,
        /// What the profile lacks.
        lacking: Lacking,
    },
    /// A threshold override its profile does not declare.
    #[error("{}: profile `{kind}` declares no threshold `{name}`", path.display())]
    UnknownThreshold {
        /// The configuration naming it.
        path: PathBuf,
        /// The profile's kind.
        kind: Kind,
        /// The threshold's name.
        name: String,
    },
    /// A rule whose engine proves no `files` case, given one.
    #[error("{}: rule `{rule}` runs on `{engine}`, which proves no `files` case", path.display())]
    FilesCase {
        /// Where the rule is declared.
        path: PathBuf,
        /// The rule.
        rule: RuleId,
        /// Its engine.
        engine: &'static str,
    },
    /// A rule on an engine proven by `files` cases, given a `code` case, which proves nothing of it.
    #[error("{}: rule `{rule}` runs on `{engine}`, a {engine} rule is proven by `files` cases, not a `code` case", path.display())]
    CodeCase {
        /// Where the rule is declared.
        path: PathBuf,
        /// The rule.
        rule: RuleId,
        /// Its engine.
        engine: &'static str,
    },
    /// A built-in catalogue name nothing provides.
    #[error("{}: no built-in catalogue is named `{name}`", path.display())]
    UnknownCatalogue {
        /// The configuration naming it.
        path: PathBuf,
        /// The name.
        name: String,
    },
    /// A value a validated type refused: a bad id, glob, selector or message, an exception without target.
    #[error("{at}: {invalid}", at = At(path, *line))]
    Invalid {
        /// The file.
        path: PathBuf,
        /// The 1-based line, when known.
        line: Option<u64>,
        /// What was refused.
        invalid: Invalid,
    },
    /// Both `law.toml` and `[tool.law]` in `pyproject.toml` in one directory.
    #[error("{}: two configurations, this and {}", law.display(), pyproject.display())]
    TwoConfigs {
        /// The `law.toml`.
        law: PathBuf,
        /// The `pyproject.toml` holding `[tool.law]`.
        pyproject: PathBuf,
    },
    /// A file that does not decode into its model.
    #[error("{at}: {message}", at = At(path, *line))]
    Parse {
        /// The file.
        path: PathBuf,
        /// The 1-based line, when known.
        line: Option<u64>,
        /// What the decoder said.
        message: String,
    },
    /// A file or directory that could not be read.
    #[error("{}: {message}", path.display())]
    Io {
        /// The file or directory.
        path: PathBuf,
        /// What the system said.
        message: String,
    },
}

struct At<'a>(&'a Path, Option<u64>);

impl fmt::Display for At<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.1 {
            Some(line) => write!(f, "{}:{line}", self.0.display()),
            None => write!(f, "{}", self.0.display()),
        }
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

    /// A YAML document that did not decode.
    #[must_use]
    pub fn yaml(path: &Path, error: &serde_saphyr::Error) -> Self {
        let line = error.location().map(|location| location.line());
        match error.without_snippet() {
            serde_saphyr::Error::Message { msg, .. } => Self::decoded(path, line, msg),
            other => Self::decoded(path, line, &other.to_string()),
        }
    }

    /// A TOML document that did not decode.
    #[must_use]
    pub fn toml(path: &Path, text: &str, error: &toml::de::Error) -> Self {
        let line = error.span().and_then(|span| Self::line(text, span.start));
        Self::decoded(path, line, error.message())
    }

    pub(crate) fn line(text: &str, offset: usize) -> Option<u64> {
        text.get(..offset)
            .map(|before| 1 + before.bytes().filter(|&byte| byte == b'\n').count() as u64)
    }

    pub(crate) fn every(first: Self, each: impl Iterator<Item = Self>) -> Vec<Self> {
        std::iter::once(first).chain(each.skip(1)).collect()
    }

    pub(crate) fn decoded(path: &Path, line: Option<u64>, message: &str) -> Self {
        message.parse::<Invalid>().map_or_else(
            |()| Self::Parse {
                path: path.to_owned(),
                line,
                message: message.to_owned(),
            },
            |invalid| Self::Invalid {
                path: path.to_owned(),
                line,
                invalid,
            },
        )
    }
}

impl Rooted for Fault {
    fn rooted(self, root: &Path) -> Self {
        match self {
            Self::DuplicateRule { id, first, second } => Self::DuplicateRule {
                id,
                first: first.rooted(root),
                second: second.rooted(root),
            },
            Self::DuplicateFormer {
                id,
                first,
                second,
                path,
            } => Self::DuplicateFormer {
                id,
                first,
                second,
                path: path.rooted(root),
            },
            Self::DuplicateProfile { kind, first, second } => Self::DuplicateProfile {
                kind,
                first: first.rooted(root),
                second: second.rooted(root),
            },
            Self::Indistinct {
                key,
                first,
                second,
                path,
            } => Self::Indistinct {
                key,
                first,
                second,
                path: path.rooted(root),
            },
            Self::UnknownKind { path, kind } => Self::UnknownKind {
                path: path.rooted(root),
                kind,
            },
            Self::UnfedAspect {
                path,
                rule,
                kind,
                aspect,
            } => Self::UnfedAspect {
                path: path.rooted(root),
                rule,
                kind,
                aspect,
            },
            Self::UnfedMeasure {
                path,
                rule,
                kind,
                measure,
                lacking,
            } => Self::UnfedMeasure {
                path: path.rooted(root),
                rule,
                kind,
                measure,
                lacking,
            },
            Self::UnknownThreshold { path, kind, name } => Self::UnknownThreshold {
                path: path.rooted(root),
                kind,
                name,
            },
            Self::FilesCase { path, rule, engine } => Self::FilesCase {
                path: path.rooted(root),
                rule,
                engine,
            },
            Self::CodeCase { path, rule, engine } => Self::CodeCase {
                path: path.rooted(root),
                rule,
                engine,
            },
            Self::UnknownCatalogue { path, name } => Self::UnknownCatalogue {
                path: path.rooted(root),
                name,
            },
            Self::Invalid { path, line, invalid } => Self::Invalid {
                path: path.rooted(root),
                line,
                invalid,
            },
            Self::TwoConfigs { law, pyproject } => Self::TwoConfigs {
                law: law.rooted(root),
                pyproject: pyproject.rooted(root),
            },
            Self::Parse { path, line, message } => Self::Parse {
                path: path.rooted(root),
                line,
                message,
            },
            Self::Io { path, message } => Self::Io {
                path: path.rooted(root),
                message,
            },
        }
    }
}
