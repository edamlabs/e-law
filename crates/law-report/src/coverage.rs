use std::collections::BTreeMap;
use std::io::{self, Write};

use law_model::{FixPolicy, Need, Prose, Rule, Tags};
use serde::Serialize;

use crate::format::Table;
use crate::rank::Rank;

const UNTAGGED: &str = "(untagged)";
const GAP: &str = "  ";
const HEADS: [&str; 4] = ["rules", "engines", "fixable (safe/unsafe/none)", "untestable"];

/// How a set of rules fixes what it finds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct Fixable {
    safe: usize,
    #[serde(rename = "unsafe")]
    risky: usize,
    none: usize,
}

/// The rules under one value of the tag: how many, on which engines, how they fix, how many `law test` cannot prove.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Row {
    value: String,
    rules: usize,
    engines: BTreeMap<&'static str, usize>,
    fixable: Fixable,
    untestable: usize,
}

/// A prose entry under the tag: its values, and why it stays prose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Stays {
    values: Vec<String>,
    text: String,
}

/// What the catalogues check, by the values of one tag key, and what they declare stays prose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Coverage {
    by: String,
    rows: Vec<Row>,
    prose: Vec<Stays>,
}

impl Row {
    fn new(value: String, rules: &[&Rule]) -> Self {
        let engines = rules.iter().fold(BTreeMap::new(), |mut engines, rule| {
            *engines.entry(rule.check.name()).or_default() += 1;
            engines
        });
        let fixed = |policy: fn(&FixPolicy) -> bool| rules.iter().filter(|rule| policy(&rule.fix)).count();
        Self {
            value,
            rules: rules.len(),
            engines,
            fixable: Fixable {
                safe: fixed(|fix| matches!(fix, FixPolicy::Safe)),
                risky: fixed(|fix| matches!(fix, FixPolicy::Unsafe)),
                none: fixed(|fix| matches!(fix, FixPolicy::None { .. })),
            },
            untestable: rules.iter().filter(|rule| Need::of(&rule.check).is_some()).count(),
        }
    }

    fn cells(&self) -> [String; 5] {
        let engines: Vec<String> = self
            .engines
            .iter()
            .map(|(engine, count)| format!("{engine} {count}"))
            .collect();
        let Fixable { safe, risky, none } = self.fixable;
        [
            self.value.clone(),
            self.rules.to_string(),
            engines.join(", "),
            format!("{safe}/{risky}/{none}"),
            self.untestable.to_string(),
        ]
    }
}

impl Coverage {
    fn values<'a>(key: &'a str, tags: &'a Tags) -> impl Iterator<Item = &'a str> {
        tags.iter()
            .filter(move |(name, _)| *name == key)
            .flat_map(|(_, values)| values)
    }

    fn new(key: &str, rules: &[&Rule], prose: &[&Prose]) -> Self {
        let mut by: BTreeMap<String, Vec<&Rule>> = rules.iter().fold(BTreeMap::new(), |by, rule| {
            Self::values(key, &rule.tags).fold(by, |mut by, value| {
                by.entry(value.to_owned()).or_default().push(*rule);
                by
            })
        });
        let untagged: Vec<&Rule> = rules
            .iter()
            .copied()
            .filter(|rule| Self::values(key, &rule.tags).next().is_none())
            .collect();
        let mut values: Vec<String> = by.keys().cloned().collect();
        Rank::sort(&mut values);
        let rows = values
            .into_iter()
            .map(|value| {
                let rules = by.remove(&value).unwrap_or_default();
                Row::new(value, &rules)
            })
            .chain((!untagged.is_empty()).then(|| Row::new(UNTAGGED.to_owned(), &untagged)))
            .collect();
        let mut stays: Vec<Stays> = prose
            .iter()
            .map(|entry| {
                let mut values: Vec<String> = Self::values(key, &entry.tags).map(str::to_owned).collect();
                Rank::sort(&mut values);
                Stays {
                    values,
                    text: entry.text.clone(),
                }
            })
            .collect();
        let mut order: Vec<String> = stays.iter().filter_map(|stays| stays.values.first().cloned()).collect();
        Rank::sort(&mut order);
        stays.sort_by_key(|stays| {
            stays
                .values
                .first()
                .and_then(|value| order.iter().position(|known| known == value))
                .unwrap_or(order.len())
        });
        Self {
            by: key.to_owned(),
            rows,
            prose: stays,
        }
    }

    fn label(&self, stays: &Stays) -> String {
        if stays.values.is_empty() {
            UNTAGGED.to_owned()
        } else {
            format!("{} {}", self.by, stays.values.join(", "))
        }
    }

    fn text(&self, out: &mut dyn Write) -> io::Result<()> {
        let head: [String; 5] = [
            self.by.clone(),
            HEADS[0].into(),
            HEADS[1].into(),
            HEADS[2].into(),
            HEADS[3].into(),
        ];
        let lines: Vec<[String; 5]> = std::iter::once(head).chain(self.rows.iter().map(Row::cells)).collect();
        let widths: Vec<usize> = (0..5)
            .map(|column| {
                lines
                    .iter()
                    .map(|line| line[column].chars().count())
                    .max()
                    .unwrap_or_default()
            })
            .collect();
        lines.iter().try_for_each(|line| {
            let cells: Vec<String> = line
                .iter()
                .zip(&widths)
                .map(|(cell, width)| format!("{cell:<width$}"))
                .collect();
            writeln!(out, "{}", cells.join(GAP).trim_end())
        })?;
        self.prose(out, "  ")
    }

    fn md(&self, out: &mut dyn Write) -> io::Result<()> {
        writeln!(out, "| {} | {} |", self.by, HEADS.join(" | "))?;
        writeln!(out, "| --- | ---: | --- | ---: | ---: |")?;
        self.rows
            .iter()
            .try_for_each(|row| writeln!(out, "| {} |", row.cells().join(" | ")))?;
        self.prose(out, "- ")
    }

    fn prose(&self, out: &mut dyn Write, bullet: &str) -> io::Result<()> {
        if self.prose.is_empty() {
            return Ok(());
        }
        writeln!(out)?;
        writeln!(out, "Stays prose:")?;
        self.prose
            .iter()
            .try_for_each(|stays| writeln!(out, "{bullet}{}: {}", self.label(stays), stays.text))
    }
}

/// The coverage of the rules by the values of one tag key — rules, engines, fixes and what `law test` cannot prove,
/// a last `(untagged)` row for the rules lacking the key — then what the catalogues declare stays prose.
///
/// # Errors
/// When the output cannot be written.
pub fn coverage(key: &str, rules: &[&Rule], prose: &[&Prose], format: Table, out: &mut dyn Write) -> io::Result<()> {
    let coverage = Coverage::new(key, rules, prose);
    match format {
        Table::Text => coverage.text(out),
        Table::Md => coverage.md(out),
        Table::Json => {
            serde_json::to_writer_pretty(&mut *out, &coverage)?;
            writeln!(out)
        }
    }
}
