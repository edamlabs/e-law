use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};

use law_model::{Edit, Finding, Safety};
use tempfile::NamedTempFile;

use crate::fault::Fault;
use crate::ground::{Disk, Ground, Memory};
use crate::plan::{Failure, Item, Plan, Splice};

/// What applying fixes did: the findings fixed, those left for the next pass, and what failed.
#[derive(Debug, Default)]
pub struct Applied {
    /// The findings whose fix was applied.
    pub fixed: Vec<Finding>,
    /// The findings whose fix overlapped one already taken; still reported, fixed on a later pass.
    pub skipped: Vec<Finding>,
    /// The faults; a finding that failed is left as it was, every file of it.
    pub faults: Vec<Fault>,
}

/// What undoes a step of a commit that did not end.
enum Undo {
    Restore(PathBuf, String),
    Remove(PathBuf),
}

/// A pass on disk, staged: each file's new text written beside its destination under a temporary name, the
/// directories made for it noted.
struct Staged {
    temps: Vec<NamedTempFile>,
    made: Vec<PathBuf>,
}

impl Staged {
    fn failure(item: &Item, fault: Fault) -> Failure {
        Failure {
            paths: vec![item.from.clone(), item.to.clone()],
            faults: vec![fault],
        }
    }

    fn dirs(dir: &Path, made: &mut Vec<PathBuf>) -> io::Result<()> {
        let missing: Vec<PathBuf> = dir
            .ancestors()
            .take_while(|above| !above.exists())
            .map(Path::to_owned)
            .collect();
        fs::create_dir_all(dir)?;
        made.extend(missing);
        Ok(())
    }

    /// One file's new text beside its destination, with the mode of the file it replaces; a file that leaves a
    /// directory is probed there too, so that removing it later is not what fails.
    fn temp(root: &Path, item: &Item, made: &mut Vec<PathBuf>) -> Result<NamedTempFile, Fault> {
        let (from, to) = (root.join(&item.from), root.join(&item.to));
        let at = |full: &Path| {
            let full = full.to_owned();
            move |error: io::Error| Fault::io(&full, &error)
        };
        let dir = to.parent().unwrap_or_else(|| Path::new("."));
        Self::dirs(dir, made).map_err(at(&to))?;
        let permissions = fs::metadata(&from).map_err(at(&from))?.permissions();
        let mut temp = NamedTempFile::new_in(dir).map_err(at(&to))?;
        temp.write_all(item.after.as_bytes()).map_err(at(&to))?;
        temp.as_file().set_permissions(permissions).map_err(at(&to))?;
        if from != to {
            NamedTempFile::new_in(from.parent().unwrap_or_else(|| Path::new("."))).map_err(at(&from))?;
        }
        Ok(temp)
    }

    fn new(root: &Path, items: &[Item]) -> Result<Self, Failure> {
        let mut staged = Self {
            temps: Vec::new(),
            made: Vec::new(),
        };
        for item in items {
            match Self::temp(root, item, &mut staged.made) {
                Ok(temp) => staged.temps.push(temp),
                Err(fault) => {
                    Self::unmake(&staged.made);
                    return Err(Self::failure(item, fault));
                }
            }
        }
        Ok(staged)
    }

    fn unmake(made: &[PathBuf]) {
        for dir in made {
            let _ = fs::remove_dir(dir);
        }
    }

    fn renamed(root: &Path, items: &[Item], temps: Vec<NamedTempFile>, undo: &mut Vec<Undo>) -> Result<(), Failure> {
        for (item, temp) in items.iter().zip(temps) {
            let to = root.join(&item.to);
            let moved = item.from != item.to;
            let landed = if moved {
                temp.persist_noclobber(&to)
            } else {
                temp.persist(&to)
            };
            landed.map_err(|error| Self::failure(item, Fault::io(&to, &error.error)))?;
            undo.push(if moved {
                Undo::Remove(to)
            } else {
                Undo::Restore(to, item.before.clone())
            });
        }
        for item in items.iter().filter(|item| item.from != item.to) {
            let from = root.join(&item.from);
            fs::remove_file(&from).map_err(|error| Self::failure(item, Fault::io(&from, &error)))?;
            undo.push(Undo::Restore(from, item.before.clone()));
        }
        Ok(())
    }

    /// The files in place: every temporary renamed onto its destination — a move onto nothing, a rewrite over the
    /// old file — and then the moved files removed from where they were; a step that fails undoes those before it.
    fn land(self, root: &Path, items: &[Item]) -> Result<(), Failure> {
        let mut undo = Vec::new();
        let landed = Self::renamed(root, items, self.temps, &mut undo);
        if landed.is_err() {
            undo.into_iter().rev().for_each(|step| {
                let _ = match step {
                    Undo::Restore(path, text) => fs::write(path, text),
                    Undo::Remove(path) => fs::remove_file(path),
                };
            });
            Self::unmake(&self.made);
        }
        landed
    }
}

/// Fixes settled against a ground and landed by `land`, until a pass lands or no finding is left to drop: a finding
/// whose files cannot be settled or landed is dropped, with its faults, and the pass settled again without it, so
/// no finding is ever half applied.
fn settle(
    findings: &[Finding],
    allow: Safety,
    ground: &impl Ground,
    mut land: impl FnMut(&[Item]) -> Result<(), Failure>,
) -> Applied {
    let mut rest = findings.to_vec();
    let mut faults = Vec::new();
    loop {
        let plan = Plan::new(&rest, allow);
        let landed = plan.settle(ground).and_then(|items| land(&items));
        let Err(failure) = landed else {
            faults.extend(plan.faults);
            return Applied {
                fixed: plan.fixed,
                skipped: plan.skipped,
                faults,
            };
        };
        faults.extend(failure.faults);
        let before = rest.len();
        rest.retain(|finding| !Plan::touches(finding, &failure.paths));
        if rest.len() == before {
            faults.extend(plan.faults);
            return Applied {
                fixed: Vec::new(),
                skipped: plan.skipped,
                faults,
            };
        }
    }
}

/// The fixes of the findings applied to their files under `root`: safety at most `allow`, overlaps left for the next
/// pass. A file is written only when it changes, each atomically; a move is staged beside its destination first and
/// renamed in, and a finding whose move or edits cannot all land — a destination that exists, a file or directory its
/// owner protected — lands none of them.
#[must_use]
pub fn apply(root: &Path, findings: &[Finding], allow: Safety) -> Applied {
    settle(findings, allow, &Disk(root), |items| {
        Staged::new(root, items)?.land(root, items)
    })
}

/// The fixes of the findings applied to a tree held in memory, by path from its root: the tree after them, moves
/// included, and what was done.
#[must_use]
pub fn project(
    files: &BTreeMap<PathBuf, String>,
    findings: &[Finding],
    allow: Safety,
) -> (BTreeMap<PathBuf, String>, Applied) {
    let mut after = files.clone();
    let applied = settle(findings, allow, &Memory(files), |items| {
        after = files.clone();
        for item in items {
            after.remove(&item.from);
        }
        for item in items {
            after.insert(item.to.clone(), item.after.clone());
        }
        Ok(())
    });
    (after, applied)
}

/// The fixes of the findings applied to one text in memory, the file at `path`; a fix that edits another file or
/// moves one is not taken.
#[must_use]
pub fn patch(path: &Path, text: &str, findings: &[Finding], allow: Safety) -> (String, Applied) {
    let within = |finding: &Finding| {
        finding.fix.iter().flat_map(|fix| &fix.edits).all(|edit| match edit {
            Edit::Replace { path: edited, .. } => edited == path,
            Edit::Create { .. } => true,
            Edit::Move { .. } => false,
        })
    };
    let local: Vec<Finding> = findings.iter().filter(|finding| within(finding)).cloned().collect();
    let mut plan = Plan::new(&local, allow);
    let splices = plan.files.remove(path).unwrap_or_default();
    match Splice::all(text.to_owned(), &splices) {
        Ok(patched) => (
            patched,
            Applied {
                fixed: plan.fixed,
                skipped: plan.skipped,
                faults: plan.faults,
            },
        ),
        Err(bytes) => {
            plan.faults.push(Fault::Range {
                path: path.to_owned(),
                bytes,
            });
            plan.fail(&[path.to_owned()]);
            (
                text.to_owned(),
                Applied {
                    fixed: plan.fixed,
                    skipped: plan.skipped,
                    faults: plan.faults,
                },
            )
        }
    }
}
