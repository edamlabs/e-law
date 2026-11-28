use law_model::{Invalid, RuleId, Severity, Tags};

use crate::engine::Author;

/// The rules the engine itself holds, outside every catalogue; selected and ignored like any rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reserved {
    /// An inline `ast-grep-ignore` pragma.
    Pragma,
    /// An exception past its expiry date.
    Expired,
    /// An exception that silenced nothing in a whole-tree run.
    Unused,
}

/// A reserved rule, ready to sign findings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservedRule {
    id: RuleId,
    message: &'static str,
    tags: Tags,
}

impl Reserved {
    fn id(self) -> &'static str {
        match self {
            Self::Pragma => "law.pragma",
            Self::Expired => "law.exception-expired",
            Self::Unused => "law.exception-unused",
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::Pragma => "silenced inline: register an exception instead",
            Self::Expired => "the exception has expired: renew it or remove it",
            Self::Unused => "the exception silences nothing: remove it",
        }
    }

    /// The rule, its id validated.
    ///
    /// # Errors
    /// Never for the ids written here; typed so a bad edit fails loudly.
    pub fn rule(self) -> Result<ReservedRule, Invalid> {
        Ok(ReservedRule {
            id: self.id().parse()?,
            message: self.message(),
            tags: Tags::default(),
        })
    }
}

impl Author for ReservedRule {
    fn id(&self) -> &RuleId {
        &self.id
    }

    fn message(&self) -> &str {
        self.message
    }

    fn severity(&self) -> Severity {
        Severity::Error
    }

    fn tags(&self) -> &Tags {
        &self.tags
    }
}
