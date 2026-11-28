use std::str::FromStr;

use law_model::{Former, Invalid, RuleId};

/// What `law rule` is asked for: a rule by its id, or by an id it went by before.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// A rule's id.
    Id(RuleId),
    /// An id a rule went by before.
    Former(Former),
}

impl FromStr for Asked {
    type Err = Invalid;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        text.parse()
            .map(Self::Id)
            .or_else(|refused| text.parse().map(Self::Former).map_err(|_| refused))
    }
}
