use law_model::{Rule, RuleId, Selector, Tags};

use crate::config::Config;

/// The rules a run checks: the configuration's selection, replaced by the command line's where given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    /// The rules selected.
    pub select: Vec<Selector>,
    /// The rules ignored; ignoring wins.
    pub ignore: Vec<Selector>,
}

impl Selection {
    /// The configuration's selection, each list replaced by the command line's when given.
    #[must_use]
    pub fn new(config: &Config, select: Option<Vec<Selector>>, ignore: Option<Vec<Selector>>) -> Self {
        Self {
            select: select.unwrap_or_else(|| config.select.clone()),
            ignore: ignore.unwrap_or_else(|| config.ignore.clone()),
        }
    }

    /// Whether a rule is checked: some selector chooses it and no ignored one does.
    #[must_use]
    pub fn admits(&self, rule: &Rule) -> bool {
        self.chooses(&rule.id, &rule.tags)
    }

    /// Whether a rule of this id and these tags is checked; the engine's reserved rules have no catalogue entry.
    #[must_use]
    pub fn chooses(&self, id: &RuleId, tags: &Tags) -> bool {
        self.select.iter().any(|selector| selector.chooses(id, tags))
            && !self.ignore.iter().any(|selector| selector.chooses(id, tags))
    }
}
