use std::fmt;
use std::str::FromStr;

/// The kind of value a validated type refuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Refusal {
    /// A rule id that is not `<namespace>.<name>`.
    BadId,
    /// An external linter's code that is not capitals then digits.
    BadCode,
    /// An entity kind that is not one lowercase word.
    BadKind,
    /// A glob that does not compile.
    BadGlob,
    /// A regular expression that does not compile.
    BadExpression,
    /// A class-name template that does not compile.
    BadTemplate,
    /// A selector that is neither `ALL`, an id, an id glob nor `key=value`.
    BadSelector,
    /// A message holding a newline.
    MultilineMessage,
    /// A text that must not be empty and is.
    EmptyText,
    /// An exception naming neither a path nor a symbol.
    ExceptionWithoutTarget,
    /// A members check asking for neither lineage nor order.
    EmptyMembers,
    /// A profile check naming no kind.
    EmptyKinds,
    /// A case that sets neither or both of `code` and `files`, an empty `files`, or `files` with a `path`.
    BadCase,
    /// Entries that set neither or both of `beside` and `glob`.
    BadEntries,
    /// A setting holding a null, which TOML cannot write.
    BadSetting,
}

const ALL: [Refusal; 15] = [
    Refusal::BadId,
    Refusal::BadCode,
    Refusal::BadKind,
    Refusal::BadGlob,
    Refusal::BadExpression,
    Refusal::BadTemplate,
    Refusal::BadSelector,
    Refusal::MultilineMessage,
    Refusal::EmptyText,
    Refusal::ExceptionWithoutTarget,
    Refusal::EmptyMembers,
    Refusal::EmptyKinds,
    Refusal::BadCase,
    Refusal::BadEntries,
    Refusal::BadSetting,
];

/// A value a validated type refused, and why.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{refusal}: {detail}")]
pub struct Invalid {
    /// What was refused.
    pub refusal: Refusal,
    /// The refused value and the reason, in words.
    pub detail: String,
}

impl Refusal {
    /// The stable code this refusal is written as.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::BadId => "bad-id",
            Self::BadCode => "bad-code",
            Self::BadKind => "bad-kind",
            Self::BadGlob => "bad-glob",
            Self::BadExpression => "bad-expression",
            Self::BadTemplate => "bad-template",
            Self::BadSelector => "bad-selector",
            Self::MultilineMessage => "multiline-message",
            Self::EmptyText => "empty-text",
            Self::ExceptionWithoutTarget => "exception-without-target",
            Self::EmptyMembers => "empty-members",
            Self::EmptyKinds => "empty-kinds",
            Self::BadCase => "bad-case",
            Self::BadEntries => "bad-entries",
            Self::BadSetting => "bad-setting",
        }
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl FromStr for Refusal {
    type Err = ();

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        ALL.into_iter().find(|refusal| refusal.code() == text).ok_or(())
    }
}

impl Invalid {
    /// A refusal of a value, with its reason.
    pub fn new(refusal: Refusal, detail: impl fmt::Display) -> Self {
        Self {
            refusal,
            detail: detail.to_string(),
        }
    }
}

impl FromStr for Invalid {
    type Err = ();

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (code, detail) = text.split_once(": ").ok_or(())?;
        Ok(Self::new(code.parse()?, detail))
    }
}
