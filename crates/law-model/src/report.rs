use std::collections::BTreeMap;
use std::path::PathBuf;

use jiff::Timestamp;
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};

use crate::finding::Finding;
use crate::id::RuleId;
use crate::selector::Selector;
use crate::severity::Severity;

/// A catalogue as a run used it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Stamp {
    /// Its name.
    pub name: String,
    /// Its version.
    pub version: String,
    /// The digest of its contents.
    pub digest: String,
}

/// An external tool as a run used it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Version {
    /// Its name.
    pub name: String,
    /// Its version, as it reports it.
    pub version: String,
}

/// What a run was: the tool, the commit, the moment, the catalogues, the selection and the paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Run {
    /// The tool's version.
    pub tool: String,
    /// The commit checked, when in a repository.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// When it ran, in RFC 3339.
    #[schemars(with = "String", extend("format" = "date-time"))]
    pub date: Timestamp,
    /// The catalogues loaded.
    pub catalogues: Vec<Stamp>,
    /// The rules selected.
    pub select: Vec<Selector>,
    /// The rules ignored.
    pub ignore: Vec<Selector>,
    /// The paths checked.
    pub paths: Vec<PathBuf>,
    /// The external tools run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Version>,
}

/// The counts of a run's findings: in all, by severity, by tag value and by rule.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Summary {
    /// Every finding.
    pub total: usize,
    /// Per severity.
    #[schemars(schema_with = "Summary::severities")]
    pub by_severity: BTreeMap<Severity, usize>,
    /// Per tag key, per value.
    pub by_tag: BTreeMap<String, BTreeMap<String, usize>>,
    /// Per rule.
    pub by_rule: BTreeMap<RuleId, usize>,
}

/// A run, its findings in order, their summary, and what each rule that fired says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Report {
    /// What the run was.
    pub run: Run,
    /// What it found, in order.
    pub findings: Vec<Finding>,
    /// Their counts.
    pub summary: Summary,
    /// The message of every rule that fired, by id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub rules: BTreeMap<RuleId, String>,
}

impl Summary {
    fn severities(generator: &mut SchemaGenerator) -> Schema {
        let mut schema = generator.subschema_for::<BTreeMap<Severity, usize>>();
        let names = generator.subschema_for::<Severity>().to_value();
        schema.insert("propertyNames".to_owned(), names);
        schema
    }
}

impl<'a> FromIterator<&'a Finding> for Summary {
    fn from_iter<I: IntoIterator<Item = &'a Finding>>(findings: I) -> Self {
        findings.into_iter().fold(Self::default(), |mut summary, finding| {
            summary.total += 1;
            *summary.by_severity.entry(finding.severity).or_default() += 1;
            *summary.by_rule.entry(finding.rule.clone()).or_default() += 1;
            finding.tags.iter().for_each(|(key, values)| {
                let counts = summary.by_tag.entry(key.to_owned()).or_default();
                values.for_each(|value| *counts.entry(value.to_owned()).or_default() += 1);
            });
            summary
        })
    }
}

impl Report {
    /// A report of a run: its findings sorted and counted.
    #[must_use]
    pub fn new(run: Run, mut findings: Vec<Finding>) -> Self {
        findings.sort();
        let summary = findings.iter().collect();
        Self {
            run,
            findings,
            summary,
            rules: BTreeMap::new(),
        }
    }
}
