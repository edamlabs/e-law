use std::io::{self, Write};

use law_model::{Finding, Report, Severity};

use crate::count::Count;
use crate::format::Output;
use crate::markdown::Markdown;
use crate::sarif::sarif;
use crate::tally::Tally;

const FINDING: &str = "finding";
const ERROR: &str = "error";
const WARNING: &str = "warning";
const FAULT: &str = "fault";

impl Output {
    fn text(report: &Report, tally: Tally, out: &mut dyn Write) -> io::Result<()> {
        report
            .findings
            .iter()
            .try_for_each(|finding| Self::line(finding, out))?;
        let count = |severity| report.summary.by_severity.get(&severity).copied().unwrap_or_default();
        let found = !report.findings.is_empty();
        if found {
            writeln!(out)?;
        }
        if let Some(fixed) = tally.fixed {
            writeln!(out, "Fixed {}.", Count(fixed, FINDING))?;
        }
        match (found, tally.faults) {
            (true, _) => writeln!(
                out,
                "Found {} ({}, {}).",
                Count(report.summary.total, FINDING),
                Count(count(Severity::Error), ERROR),
                Count(count(Severity::Warning), WARNING)
            )?,
            (false, 0) => writeln!(out, "All checks passed.")?,
            (false, _) => {}
        }
        if tally.faults > 0 {
            writeln!(out, "{}: the run is incomplete.", Count(tally.faults, FAULT))?;
        }
        Ok(())
    }

    fn line(finding: &Finding, out: &mut dyn Write) -> io::Result<()> {
        writeln!(
            out,
            "{}:{}:{}: {} {}",
            finding.path.display(),
            finding.span.start.line,
            finding.span.start.column,
            finding.rule,
            finding.message
        )
    }

    fn json(report: &Report, out: &mut dyn Write) -> io::Result<()> {
        serde_json::to_writer_pretty(&mut *out, report)?;
        writeln!(out)
    }
}

/// A report written in a format; in text, closed by its tally; in SARIF and Markdown, the report alone.
///
/// # Errors
/// When the output cannot be written.
pub fn write(report: &Report, tally: Tally, format: Output, out: &mut dyn Write) -> io::Result<()> {
    match format {
        Output::Text => Output::text(report, tally, out),
        Output::Json => Output::json(report, out),
        Output::Sarif => sarif(report, out),
        Output::Md => Markdown::run(report, out),
    }
}
