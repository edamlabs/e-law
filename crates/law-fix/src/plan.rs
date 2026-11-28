use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::{Path, PathBuf};

use law_model::{Edit, Finding, Fix, Safety};

use crate::fault::Fault;
use crate::ground::Ground;

const READ_ONLY: &str = "read-only: a fixer does not write what its owner protected";

/// One byte range of a file and what replaces it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Splice {
    pub(crate) bytes: Range<usize>,
    pub(crate) text: String,
}

/// One file as a pass leaves it: where it was and where it lands, what it held and what it holds.
#[derive(Debug)]
pub(crate) struct Item {
    pub(crate) from: PathBuf,
    pub(crate) to: PathBuf,
    pub(crate) before: String,
    pub(crate) after: String,
}

/// Why a pass could not be settled: the paths that failed and the faults.
#[derive(Debug, Default)]
pub(crate) struct Failure {
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) faults: Vec<Fault>,
}

/// The edits of a fix: splices of files, by the path they have before the pass, and moves of files; none when it
/// asks for a file to be created.
struct Parts {
    splices: Vec<(PathBuf, Splice)>,
    moves: Vec<(PathBuf, PathBuf)>,
}

/// The fixes taken from a set of findings: no two edits of a file overlap, the first by position wins; a file moves
/// once and a path is landed on once. Every edit speaks of the paths as the check saw them, a moved file's too.
#[derive(Debug, Default)]
pub(crate) struct Plan {
    pub(crate) files: BTreeMap<PathBuf, Vec<Splice>>,
    pub(crate) moves: BTreeMap<PathBuf, PathBuf>,
    pub(crate) fixed: Vec<Finding>,
    pub(crate) skipped: Vec<Finding>,
    pub(crate) faults: Vec<Fault>,
}

#[derive(Default)]
struct Taken {
    ranges: BTreeMap<PathBuf, BTreeMap<usize, usize>>,
    from: BTreeSet<PathBuf>,
    to: BTreeSet<PathBuf>,
}

impl Taken {
    fn conflicts(taken: Option<&BTreeMap<usize, usize>>, bytes: &Range<usize>) -> bool {
        taken.is_some_and(|taken| {
            let before = taken.range(..=bytes.start).next_back();
            let after = taken.range(bytes.start..).next();
            before.is_some_and(|(&start, &end)| start == bytes.start || end > bytes.start)
                || after.is_some_and(|(&start, _)| start == bytes.start || start < bytes.end)
        })
    }

    fn fits(&self, parts: &Parts) -> bool {
        let mut local = Self::default();
        let spliced = parts.splices.iter().all(|(path, splice)| {
            let free = !Self::conflicts(self.ranges.get(path), &splice.bytes)
                && !Self::conflicts(local.ranges.get(path), &splice.bytes);
            local
                .ranges
                .entry(path.clone())
                .or_default()
                .insert(splice.bytes.start, splice.bytes.end);
            free
        });
        let moved = parts.moves.iter().all(|(from, to)| {
            let free = !self.from.contains(from)
                && !self.to.contains(to)
                && !local.from.contains(from)
                && !local.to.contains(to);
            local.from.insert(from.clone());
            local.to.insert(to.clone());
            free
        });
        spliced && moved
    }

    fn take(&mut self, parts: &Parts) {
        parts.splices.iter().for_each(|(path, splice)| {
            self.ranges
                .entry(path.clone())
                .or_default()
                .insert(splice.bytes.start, splice.bytes.end);
        });
        parts.moves.iter().for_each(|(from, to)| {
            self.from.insert(from.clone());
            self.to.insert(to.clone());
        });
    }
}

impl Parts {
    fn of(fix: &Fix) -> Option<Self> {
        let mut parts = Self {
            splices: Vec::new(),
            moves: Vec::new(),
        };
        for edit in &fix.edits {
            match edit {
                Edit::Replace { path, bytes, text } => parts.splices.push((
                    path.clone(),
                    Splice {
                        bytes: bytes.clone(),
                        text: text.clone(),
                    },
                )),
                Edit::Move { from, to } => parts.moves.push((from.clone(), to.clone())),
                Edit::Create { .. } => return None,
            }
        }
        Some(parts)
    }
}

impl Plan {
    fn take(mut self, taken: &mut Taken, finding: &Finding, fix: &Fix) -> Self {
        match Parts::of(fix) {
            None => self.faults.push(Fault::Unsupported {
                rule: finding.rule.clone(),
            }),
            Some(parts) if taken.fits(&parts) => {
                taken.take(&parts);
                for (path, splice) in parts.splices {
                    self.files.entry(path).or_default().push(splice);
                }
                self.moves.extend(parts.moves);
                self.fixed.push(finding.clone());
            }
            Some(_) => self.skipped.push(finding.clone()),
        }
        self
    }

    /// The fixes of the findings whose safety is at most `allow`, taken in order of their first edit.
    pub(crate) fn new(findings: &[Finding], allow: Safety) -> Self {
        let mut eligible: Vec<(&Finding, &Fix)> = findings
            .iter()
            .filter_map(|finding| finding.fix.as_ref().map(|fix| (finding, fix)))
            .filter(|(_, fix)| fix.safety <= allow)
            .collect();
        eligible.sort_by(|(_, a), (_, b)| a.edits.iter().min().cmp(&b.edits.iter().min()));
        let mut taken = Taken::default();
        eligible.into_iter().fold(Self::default(), |plan, (finding, fix)| {
            plan.take(&mut taken, finding, fix)
        })
    }

    /// Whether a finding's fix touches any of the paths: an edit of one, or a move from or to one.
    pub(crate) fn touches(finding: &Finding, failed: &[PathBuf]) -> bool {
        finding.fix.iter().flat_map(|fix| &fix.edits).any(|edit| match edit {
            Edit::Replace { path, .. } | Edit::Create { path, .. } => failed.contains(path),
            Edit::Move { from, to } => failed.contains(from) || failed.contains(to),
        })
    }

    /// The fixed findings that touch none of the files that failed.
    pub(crate) fn fail(&mut self, failed: &[PathBuf]) {
        self.fixed.retain(|finding| !Self::touches(finding, failed));
    }

    fn item(&self, ground: &impl Ground, from: &Path) -> Result<Option<Item>, Failure> {
        let fail = |paths: &[&Path], fault: Fault| Failure {
            paths: paths.iter().map(|path| (*path).to_owned()).collect(),
            faults: vec![fault],
        };
        let splices = self.files.get(from).map_or(&[][..], Vec::as_slice);
        let to = self.moves.get(from).map_or(from, PathBuf::as_path);
        let before = ground.read(from).map_err(|fault| fail(&[from], fault))?;
        let after = Splice::all(before.clone(), splices).map_err(|bytes| {
            fail(
                &[from],
                Fault::Range {
                    path: from.to_owned(),
                    bytes,
                },
            )
        })?;
        let moved = to != from;
        if after == before && !moved {
            return Ok(None);
        }
        if !ground.writable(from) {
            return Err(fail(
                &[from],
                Fault::Io {
                    path: ground.locate(from),
                    message: READ_ONLY.to_owned(),
                },
            ));
        }
        if moved && ground.exists(to) {
            return Err(fail(&[to, from], Fault::Occupied { path: to.to_owned() }));
        }
        Ok(Some(Item {
            from: from.to_owned(),
            to: to.to_owned(),
            before,
            after,
        }))
    }

    /// Every file the plan changes as the pass leaves it, or what stops it: each file read, its splices applied to
    /// what it held, a moved one checked to land on nothing; every failure of the pass met, not the first.
    pub(crate) fn settle(&self, ground: &impl Ground) -> Result<Vec<Item>, Failure> {
        let paths: BTreeSet<&PathBuf> = self.files.keys().chain(self.moves.keys()).collect();
        let (items, failures): (Vec<_>, Vec<_>) = paths
            .into_iter()
            .map(|path| self.item(ground, path))
            .partition(Result::is_ok);
        let failure = failures
            .into_iter()
            .filter_map(Result::err)
            .fold(Failure::default(), |mut all, one| {
                all.paths.extend(one.paths);
                all.faults.extend(one.faults);
                all
            });
        if failure.faults.is_empty() {
            Ok(items.into_iter().flatten().flatten().collect())
        } else {
            Err(failure)
        }
    }
}

impl Splice {
    /// The text with every splice applied, back to front; the bytes of the first that falls outside it or splits a
    /// character when one does.
    pub(crate) fn all(mut text: String, splices: &[Self]) -> Result<String, Range<usize>> {
        let mut ordered: Vec<&Self> = splices.iter().collect();
        ordered.sort_by_key(|splice| std::cmp::Reverse(splice.bytes.start));
        ordered.into_iter().try_for_each(|splice| {
            let Range { start, end } = splice.bytes;
            let fits = start <= end && text.is_char_boundary(start) && text.is_char_boundary(end);
            fits.then(|| text.replace_range(start..end, &splice.text))
                .ok_or_else(|| splice.bytes.clone())
        })?;
        Ok(text)
    }
}
