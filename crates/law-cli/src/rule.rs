use law_model::{Rule, RuleId};

use crate::asked::Asked;
use crate::cli::RuleArgs;
use crate::exit::Exit;
use crate::project::Opened;

const NEAREST: usize = 5;

struct Nearest;

impl Nearest {
    fn distance(a: &str, b: &str) -> usize {
        let b: Vec<char> = b.chars().collect();
        let first: Vec<usize> = (0..=b.len()).collect();
        let last = a.chars().enumerate().fold(first, |previous, (i, left)| {
            b.iter().enumerate().fold(vec![i + 1], |mut row, (j, right)| {
                let cost = usize::from(left != *right);
                let value = (previous[j] + cost).min(previous[j + 1] + 1).min(row[j] + 1);
                row.push(value);
                row
            })
        });
        last.last().copied().unwrap_or_default()
    }

    fn of<'a>(id: &RuleId, rules: impl Iterator<Item = &'a Rule>) -> String {
        let mut near: Vec<(usize, &RuleId)> = rules
            .filter(|rule| rule.id.namespace() == id.namespace())
            .map(|rule| (Self::distance(id.name(), rule.id.name()), &rule.id))
            .collect();
        near.sort();
        let listed: Vec<String> = near.iter().take(NEAREST).map(|(_, id)| id.to_string()).collect();
        if listed.is_empty() {
            format!("rule `{id}` not found; no rule in namespace `{}`", id.namespace())
        } else {
            format!("rule `{id}` not found; nearest: {}", listed.join(", "))
        }
    }
}

impl RuleArgs {
    fn one(&self, opened: &Opened, asked: &Asked) -> Result<(), Exit> {
        let rules = || opened.loaded.catalogues.iter().flat_map(|catalogue| &catalogue.rules);
        let found = match asked {
            Asked::Id(id) => rules()
                .find(|rule| &rule.id == id)
                .ok_or_else(|| Exit::faults([Nearest::of(id, rules())]))?,
            Asked::Former(old) => rules()
                .find(|rule| rule.formerly.contains(old))
                .ok_or_else(|| Exit::faults([format!("rule `{old}` not found; no rule went by it")]))?,
        };
        Exit::emit(|out| law_report::rule(found, self.output_format, out))
    }

    fn all(&self, opened: &Opened) -> Result<(), Exit> {
        let selected: Vec<&Rule> = opened
            .loaded
            .catalogues
            .iter()
            .flat_map(|catalogue| &catalogue.rules)
            .filter(|rule| opened.selection.admits(rule))
            .collect();
        Exit::emit(|out| law_report::rules(&selected, self.output_format, out))
    }

    fn shown(&self) -> Result<Exit, Exit> {
        let opened = self.project.open(&self.catalogues)?;
        self.id
            .as_ref()
            .map_or_else(|| self.all(&opened), |id| self.one(&opened, id))?;
        Ok(Exit::Clean)
    }

    /// The rule shown, or every selected rule listed; an unknown id is a fault naming the nearest ids.
    #[must_use]
    pub fn run(&self) -> Exit {
        self.shown().unwrap_or_else(|exit| exit)
    }
}
