use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};

use law_model::{Finding, Report, RuleId};

use crate::count::Count;
use crate::rank::Rank;

const CAP: usize = 20;
const LAW: &str = "law";
const TOOL: &str = "law";
const FINDING: &str = "finding";
const RULE: &str = "rule";
const UNTAGGED: &str = "(untagged)";

/// A report as Markdown a person reads in a pull request.
pub(crate) struct Markdown<'a> {
    report: &'a Report,
}

impl Markdown<'_> {
    fn head(&self, out: &mut dyn Write) -> io::Result<()> {
        let run = &self.report.run;
        let catalogues: Vec<String> = run
            .catalogues
            .iter()
            .map(|stamp| format!("{} {}", stamp.name, stamp.version))
            .collect();
        let parts: Vec<String> = [
            Some(format!("{TOOL} {}", run.tool)),
            run.commit.as_ref().map(|commit| format!("commit {commit}")),
            Some(run.date.to_string()),
            (!catalogues.is_empty()).then(|| format!("catalogues: {}", catalogues.join(", "))),
        ]
        .into_iter()
        .flatten()
        .collect();
        writeln!(out, "{}", parts.join(" · "))?;
        writeln!(out)?;
        writeln!(
            out,
            "{}, {} hit.",
            Count(self.report.summary.total, FINDING),
            Count(self.report.summary.by_rule.len(), RULE)
        )
    }

    fn keys(&self) -> Vec<&str> {
        let keys: BTreeSet<&str> = self.report.summary.by_tag.keys().map(String::as_str).collect();
        keys.contains(LAW)
            .then_some(LAW)
            .into_iter()
            .chain(keys.iter().copied().filter(|key| *key != LAW))
            .collect()
    }

    fn table(&self, key: &str, out: &mut dyn Write) -> io::Result<()> {
        let counts = self.report.summary.by_tag.get(key);
        let mut hit: BTreeMap<&str, BTreeSet<&RuleId>> = BTreeMap::new();
        self.report.findings.iter().for_each(|finding| {
            finding
                .tags
                .iter()
                .filter(|(name, _)| *name == key)
                .flat_map(|(_, values)| values)
                .for_each(|value| {
                    hit.entry(value).or_default().insert(&finding.rule);
                });
        });
        let mut values: Vec<String> = counts.into_iter().flat_map(BTreeMap::keys).cloned().collect();
        Rank::sort(&mut values);
        writeln!(out)?;
        writeln!(out, "| {key} | findings | rules hit |")?;
        writeln!(out, "| --- | ---: | ---: |")?;
        values.iter().try_for_each(|value| {
            let found = counts.and_then(|counts| counts.get(value)).copied().unwrap_or_default();
            let rules = hit.get(value.as_str()).map_or(0, BTreeSet::len);
            writeln!(out, "| {value} | {found} | {rules} |")
        })?;
        let untagged: Vec<&Finding> = self
            .report
            .findings
            .iter()
            .filter(|finding| !finding.tags.iter().any(|(name, _)| name == key))
            .collect();
        let rules: BTreeSet<&RuleId> = untagged.iter().map(|finding| &finding.rule).collect();
        match untagged.len() {
            0 => Ok(()),
            found => writeln!(out, "| {UNTAGGED} | {found} | {} |", rules.len()),
        }
    }

    fn rule(&self, rule: &RuleId, findings: &[&Finding], out: &mut dyn Write) -> io::Result<()> {
        let message = self.report.rules.get(rule).map_or("", String::as_str);
        writeln!(out)?;
        writeln!(out, "### `{rule}` ({})", findings.len())?;
        writeln!(out)?;
        if !message.is_empty() {
            writeln!(out, "{message}")?;
            writeln!(out)?;
        }
        findings
            .iter()
            .take(CAP)
            .try_for_each(|finding| writeln!(out, "- `{}:{}`", finding.path.display(), finding.span.start.line))?;
        match findings.len().checked_sub(CAP) {
            Some(more) if more > 0 => writeln!(out, "- … and {more} more"),
            _ => Ok(()),
        }
    }

    fn write(&self, out: &mut dyn Write) -> io::Result<()> {
        self.head(out)?;
        if self.report.findings.is_empty() {
            return Ok(());
        }
        writeln!(out)?;
        writeln!(out, "## Summary")?;
        self.keys().into_iter().try_for_each(|key| self.table(key, out))?;
        let mut grouped: BTreeMap<&RuleId, Vec<&Finding>> = BTreeMap::new();
        self.report.findings.iter().for_each(|finding| {
            grouped.entry(&finding.rule).or_default().push(finding);
        });
        writeln!(out)?;
        writeln!(out, "## Findings")?;
        grouped
            .iter()
            .try_for_each(|(rule, findings)| self.rule(rule, findings, out))
    }

    /// The report written as Markdown: a header line, a summary table per tag key, its last row the findings of rules
    /// without that key, then the findings by rule.
    ///
    /// # Errors
    /// When the output cannot be written.
    pub(crate) fn run(report: &Report, out: &mut dyn Write) -> io::Result<()> {
        Markdown { report }.write(out)
    }
}
