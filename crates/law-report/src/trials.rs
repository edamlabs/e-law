use std::io::{self, Write};

use law_model::{Trial, Trials};

use crate::count::Count;
use crate::format::Format;

const RULE: &str = "rule";
const CASE: &str = "case";

struct Verdict;

impl Verdict {
    fn line(trial: &Trial, out: &mut dyn Write) -> io::Result<()> {
        let detail = trial.detail.as_deref().unwrap_or_default();
        match trial.index {
            Some(index) => writeln!(out, "{}: {} case {index}: {detail}", trial.rule, trial.kind.name()),
            None => writeln!(out, "{}: {}: {detail}", trial.rule, trial.kind.name()),
        }
    }

    fn text(trials: &Trials, out: &mut dyn Write) -> io::Result<()> {
        trials
            .trials
            .iter()
            .filter(|trial| !trial.pass)
            .try_for_each(|trial| Self::line(trial, out))?;
        trials
            .untested
            .iter()
            .try_for_each(|untested| writeln!(out, "{}: untested: {}", untested.rule, untested.need.why()))?;
        if trials.failed > 0 || !trials.untested.is_empty() {
            writeln!(out)?;
        }
        writeln!(
            out,
            "Tested {}, {}: {} failed, {} untested.",
            Count(trials.rules, RULE),
            Count(trials.cases, CASE),
            trials.failed,
            trials.untested.len()
        )
    }
}

/// The trials of `law test` written in a format: in text, one line per failure, one per untested rule, then the
/// counts.
///
/// # Errors
/// When the output cannot be written.
pub fn trials(trials: &Trials, format: Format, out: &mut dyn Write) -> io::Result<()> {
    match format {
        Format::Text => Verdict::text(trials, out),
        Format::Json => {
            serde_json::to_writer_pretty(&mut *out, trials)?;
            writeln!(out)
        }
    }
}
