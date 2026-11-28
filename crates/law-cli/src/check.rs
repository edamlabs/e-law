use std::io::{self, Write};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use jiff::{Timestamp, ZonedDateTime};
use law_engine::{Census, Environment, Extent, Fault, Outcome, Run, Timing};
use law_model::{Edit, Finding, Safety, Severity};
use law_report::Tally;

use crate::cli::CheckArgs;
use crate::commit::Commit;
use crate::exit::Exit;
use crate::project::Opened;

const PASSES: usize = 10;
const CENSUS: &str = "members:";
const PROFILE: &str = "profile:";
const METRIC: &str = "metric:";
const TIME: &str = "time:";
const GRAPH: &str = "graph:";
const PROBE: &str = "probe";
const MILLIS: f64 = 1000.0;

impl CheckArgs {
    fn followed(root: &Path, paths: &mut [PathBuf], applied: &[Finding]) {
        let moves: Vec<(&PathBuf, &PathBuf)> = applied
            .iter()
            .filter_map(|finding| finding.fix.as_ref())
            .flat_map(|fix| &fix.edits)
            .filter_map(|edit| match edit {
                Edit::Move { from, to } => Some((from, to)),
                Edit::Replace { .. } | Edit::Create { .. } => None,
            })
            .collect();
        for path in paths.iter_mut() {
            if let Some((_, to)) = moves.iter().find(|(from, _)| root.join(from) == *path) {
                *path = root.join(to);
            }
        }
    }

    fn fixed(&self, run: &Run, opened: &Opened, paths: &[PathBuf], at: Timestamp) -> (Outcome, usize, Vec<Fault>) {
        let allow = if self.unsafe_fixes {
            Safety::Unsafe
        } else {
            Safety::Safe
        };
        let mut fixed = 0;
        let mut faults = Vec::new();
        let mut paths = paths.to_vec();
        let last = (0..PASSES).try_for_each(|_| {
            let outcome = run.check(&paths, at);
            let applied = law_fix::apply(opened.root(), &outcome.report.findings, allow);
            fixed += applied.fixed.len();
            Self::followed(opened.root(), &mut paths, &applied.fixed);
            let failed = !applied.faults.is_empty();
            faults.extend(applied.faults.into_iter().map(Fault::Fix));
            match (applied.fixed.is_empty(), failed) {
                (true, _) => ControlFlow::Break(outcome),
                (false, true) => ControlFlow::Break(run.check(&paths, at)),
                (false, false) => ControlFlow::Continue(()),
            }
        });
        let outcome = match last {
            ControlFlow::Break(outcome) => outcome,
            ControlFlow::Continue(()) => run.check(&paths, at),
        };
        faults.extend(
            outcome
                .declined
                .iter()
                .filter(|(safety, _)| *safety <= allow)
                .map(|(_, fault)| fault.clone()),
        );
        (outcome, fixed, faults)
    }

    fn census(census: &Census, environment: &Environment) {
        let mut err = io::stderr().lock();
        let _ = writeln!(err, "{CENSUS} environment: {environment}");
        let _ = writeln!(
            err,
            "{CENSUS} {} classes judged, {} skipped (unresolved base), {} on a builtin base; modules read: {} project, {} foreign",
            census.judged, census.skipped, census.builtin, census.project, census.foreign
        );
        let mut unresolved: Vec<_> = census.unresolved.iter().collect();
        unresolved.sort_by(|(a, (_, x)), (b, (_, y))| y.cmp(x).then_with(|| a.cmp(b)));
        for (name, (why, count)) in unresolved {
            let _ = writeln!(err, "{CENSUS} unresolved: {name} \u{d7}{count} ({why})");
        }
        if let Some(metric) = census.metric {
            let _ = writeln!(err, "{METRIC} {} modules measured", metric.modules);
        }
        if let Some(profile) = census.profile {
            let _ = writeln!(
                err,
                "{PROFILE} {} classes judged, {} skipped (unresolved base)",
                profile.judged, profile.skipped
            );
        }
    }

    fn graph(extent: Extent) {
        let _ = writeln!(
            io::stderr().lock(),
            "{GRAPH} {} modules, {} edges ({} typing-only)",
            extent.modules,
            extent.edges,
            extent.typing
        );
    }

    fn timings(timings: &[Timing]) {
        let mut err = io::stderr().lock();
        for timing in timings {
            let millis = timing.elapsed.as_secs_f64() * MILLIS;
            let _ = writeln!(err, "{TIME} {} {millis:.1}", timing.phase);
        }
    }

    fn checked(&self, verbose: bool) -> Result<Exit, Exit> {
        let opened = self.project.open(&[])?;
        let now = ZonedDateTime::now();
        let started = Instant::now();
        let batch = opened.batch()?;
        let probe: Duration = started.elapsed();
        let environment = Environment::discover(&opened.loaded.project.root);
        let run = Run::new(&opened.loaded, &opened.selection, now.date(), batch, &environment)
            .map_err(|faults| opened.faults(faults))?;
        let paths: Vec<PathBuf> = self.paths.iter().map(|path| opened.cwd.join(path)).collect();
        let (mut outcome, fixed, mut faults) = if self.fix {
            let (outcome, fixed, faults) = self.fixed(&run, &opened, &paths, now.timestamp());
            (outcome, (fixed > 0).then_some(fixed), faults)
        } else {
            (run.check(&paths, now.timestamp()), None, Vec::new())
        };
        outcome.report.run.commit = Commit::head(opened.root());
        faults.splice(0..0, std::mem::take(&mut outcome.faults));
        let tally = Tally {
            fixed,
            faults: faults.len(),
        };
        let format = self.output_format;
        Exit::emit(|out| law_report::write(&outcome.report, tally, format, out))?;
        if verbose {
            Self::census(&outcome.census, &environment);
            outcome.graph.into_iter().for_each(Self::graph);
            Self::timings(&[Timing::new(PROBE, probe)]);
            Self::timings(&outcome.timings);
        }
        let failed = outcome
            .report
            .findings
            .iter()
            .any(|finding| finding.severity == Severity::Error);
        match (faults.is_empty(), failed) {
            (false, _) => Err(opened.faults(faults)),
            (true, true) => Ok(Exit::Findings),
            (true, false) => Ok(Exit::Clean),
        }
    }

    /// The check, composed: open, plan, run (and fix, check again, until no fix applies), write; faults to stderr,
    /// and, verbose, what the run counted.
    #[must_use]
    pub fn run(&self, verbose: bool) -> Exit {
        self.checked(verbose).unwrap_or_else(|exit| exit)
    }
}
