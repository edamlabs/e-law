use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::{Component, Path};

use law_model::{Edit, Finding, Report, RuleId, Severity, Span};
use serde::Serialize;

const SCHEMA: &str = "https://json.schemastore.org/sarif-2.1.0.json";
const VERSION: &str = "2.1.0";
const TOOL: &str = "law";
const COLUMNS: &str = "unicodeCodePoints";
const UNRESERVED: &[u8] = b"-._~/";
const EQUALS: char = '=';
const SEPARATOR: char = '/';

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Log {
    #[serde(rename = "$schema")]
    schema: &'static str,
    version: &'static str,
    runs: [Run; 1],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Run {
    tool: Tool,
    column_kind: &'static str,
    results: Vec<Outcome>,
}

#[derive(Serialize)]
struct Tool {
    driver: Driver,
}

#[derive(Serialize)]
struct Driver {
    name: &'static str,
    version: String,
    rules: Vec<Descriptor>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Descriptor {
    id: String,
    short_description: Text,
    default_configuration: Configuration,
    properties: Properties,
}

#[derive(Serialize)]
struct Text {
    text: String,
}

#[derive(Serialize)]
struct Configuration {
    level: &'static str,
}

#[derive(Serialize)]
struct Properties {
    tags: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Outcome {
    rule_id: String,
    level: &'static str,
    message: Text,
    locations: [Location; 1],
    #[serde(skip_serializing_if = "Vec::is_empty")]
    fixes: Vec<Change>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Location {
    physical_location: Physical,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Physical {
    artifact_location: Artifact,
    region: Region,
}

#[derive(Serialize)]
struct Artifact {
    uri: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Region {
    start_line: u32,
    start_column: u32,
    end_line: u32,
    end_column: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Change {
    artifact_changes: Vec<Artifacts>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Artifacts {
    artifact_location: Artifact,
    replacements: Vec<Replacement>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Replacement {
    deleted_region: Bytes,
    inserted_content: Text,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Bytes {
    byte_offset: usize,
    byte_length: usize,
}

impl Artifact {
    fn new(path: &Path) -> Self {
        let parts: Vec<String> = path
            .components()
            .filter_map(|part| match part {
                Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect();
        let uri = parts
            .join(&SEPARATOR.to_string())
            .bytes()
            .map(|byte| {
                if byte.is_ascii_alphanumeric() || UNRESERVED.contains(&byte) {
                    char::from(byte).to_string()
                } else {
                    format!("%{byte:02X}")
                }
            })
            .collect();
        Self { uri }
    }
}

impl Region {
    fn new(span: &Span) -> Self {
        Self {
            start_line: span.start.line,
            start_column: span.start.column,
            end_line: span.end.line,
            end_column: span.end.column,
        }
    }
}

impl Outcome {
    fn level(severity: Severity) -> &'static str {
        match severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }

    fn fixes(finding: &Finding) -> Vec<Change> {
        let mut files: BTreeMap<&Path, Vec<Replacement>> = BTreeMap::new();
        finding
            .fix
            .iter()
            .flat_map(|fix| &fix.edits)
            .for_each(|edit| match edit {
                Edit::Replace { path, bytes, text } => files.entry(path).or_default().push(Replacement {
                    deleted_region: Bytes {
                        byte_offset: bytes.start,
                        byte_length: bytes.len(),
                    },
                    inserted_content: Text { text: text.clone() },
                }),
                Edit::Create { .. } | Edit::Move { .. } => {}
            });
        let changes: Vec<Artifacts> = files
            .into_iter()
            .map(|(path, replacements)| Artifacts {
                artifact_location: Artifact::new(path),
                replacements,
            })
            .collect();
        (!changes.is_empty())
            .then_some(Change {
                artifact_changes: changes,
            })
            .into_iter()
            .collect()
    }

    fn new(finding: &Finding) -> Self {
        Self {
            rule_id: finding.rule.to_string(),
            level: Self::level(finding.severity),
            message: Text {
                text: finding.message.clone(),
            },
            locations: [Location {
                physical_location: Physical {
                    artifact_location: Artifact::new(&finding.path),
                    region: Region::new(&finding.span),
                },
            }],
            fixes: Self::fixes(finding),
        }
    }
}

impl Descriptor {
    fn new(report: &Report, id: &RuleId, finding: &Finding) -> Self {
        Self {
            id: id.to_string(),
            short_description: Text {
                text: report.rules.get(id).cloned().unwrap_or_else(|| id.to_string()),
            },
            default_configuration: Configuration {
                level: Outcome::level(finding.severity),
            },
            properties: Properties {
                tags: finding
                    .tags
                    .iter()
                    .flat_map(|(key, values)| values.map(move |value| format!("{key}{EQUALS}{value}")))
                    .collect(),
            },
        }
    }
}

/// A report written as a SARIF 2.1.0 log: one run, the rules that fired, one result per finding.
///
/// # Errors
/// When the output cannot be written.
pub(crate) fn sarif(report: &Report, out: &mut dyn Write) -> io::Result<()> {
    let mut first: BTreeMap<&RuleId, &Finding> = BTreeMap::new();
    report.findings.iter().for_each(|finding| {
        first.entry(&finding.rule).or_insert(finding);
    });
    let log = Log {
        schema: SCHEMA,
        version: VERSION,
        runs: [Run {
            tool: Tool {
                driver: Driver {
                    name: TOOL,
                    version: report.run.tool.clone(),
                    rules: first
                        .into_iter()
                        .map(|(id, finding)| Descriptor::new(report, id, finding))
                        .collect(),
                },
            },
            column_kind: COLUMNS,
            results: report.findings.iter().map(Outcome::new).collect(),
        }],
    };
    serde_json::to_writer_pretty(&mut *out, &log)?;
    writeln!(out)
}
