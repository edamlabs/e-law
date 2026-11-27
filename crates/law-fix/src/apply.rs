use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use law_model::{Finding, Safety};
use tempfile::NamedTempFile;

use crate::fault::Fault;
use crate::plan::{Plan, Splice};

const READ_ONLY: &str = "read-only: a fixer does not write what its owner protected";

/// What applying fixes did: the findings fixed, those left for the next pass, and what failed.
#[derive(Debug, Default)]
pub struct Applied {
    /// The findings whose fix was applied.
    pub fixed: Vec<Finding>,
    /// The findings whose fix overlapped one already taken; still reported, fixed on a later pass.
    pub skipped: Vec<Finding>,
    /// The faults; a file that failed is left as it was.
    pub faults: Vec<Fault>,
}

impl Applied {
    fn write(full: &Path, text: &str) -> Result<(), Fault> {
        let io = |error: std::io::Error| Fault::io(full, &error);
        let permissions = fs::metadata(full).map_err(io)?.permissions();
        if permissions.readonly() {
            return Err(Fault::Io {
                path: full.to_owned(),
                message: READ_ONLY.to_owned(),
            });
        }
        let dir = full.parent().unwrap_or_else(|| Path::new("."));
        let mut temp = NamedTempFile::new_in(dir).map_err(io)?;
        temp.write_all(text.as_bytes()).map_err(io)?;
        temp.as_file().set_permissions(permissions).map_err(io)?;
        temp.persist(full).map_err(|error| io(error.error))?;
        Ok(())
    }

    fn splice(path: &Path, text: String, splices: &[Splice]) -> Result<String, Fault> {
        Splice::all(text, splices).map_err(|bytes| Fault::Range {
            path: path.to_owned(),
            bytes,
        })
    }

    fn file(root: &Path, path: &Path, splices: &[Splice]) -> Result<(), Fault> {
        let full = root.join(path);
        let before = fs::read_to_string(&full).map_err(|error| Fault::io(&full, &error))?;
        let after = Self::splice(path, before.clone(), splices)?;
        if after == before {
            return Ok(());
        }
        Self::write(&full, &after)
    }

    fn of(mut plan: Plan, failed: &[PathBuf]) -> Self {
        plan.fail(failed);
        Self {
            fixed: plan.fixed,
            skipped: plan.skipped,
            faults: plan.faults,
        }
    }
}

/// The fixes of the findings applied to their files under `root`: safety at most `allow`, overlaps left for the next
/// pass, each file rewritten once, atomically, and only when it changes.
#[must_use]
pub fn apply(root: &Path, findings: &[Finding], allow: Safety) -> Applied {
    let mut plan = Plan::new(findings, allow);
    let results: Vec<(PathBuf, Result<(), Fault>)> = plan
        .files
        .iter()
        .map(|(path, splices)| (path.clone(), Applied::file(root, path, splices)))
        .collect();
    let (failed, faults): (Vec<PathBuf>, Vec<Fault>) = results
        .into_iter()
        .filter_map(|(path, result)| result.err().map(|fault| (path, fault)))
        .unzip();
    plan.faults.extend(faults);
    Applied::of(plan, &failed)
}

/// The fixes of the findings applied to one text in memory, the file at `path`; edits of other files are not taken.
#[must_use]
pub fn patch(path: &Path, text: &str, findings: &[Finding], allow: Safety) -> (String, Applied) {
    let mut plan = Plan::new(findings, allow);
    let splices = plan.files.remove(path).unwrap_or_default();
    let others: Vec<PathBuf> = plan.files.keys().cloned().collect();
    let patched = Applied::splice(path, text.to_owned(), &splices);
    match patched {
        Ok(patched) => (patched, Applied::of(plan, &others)),
        Err(fault) => {
            plan.faults.push(fault);
            let failed: Vec<PathBuf> = others.into_iter().chain([path.to_owned()]).collect();
            (text.to_owned(), Applied::of(plan, &failed))
        }
    }
}
