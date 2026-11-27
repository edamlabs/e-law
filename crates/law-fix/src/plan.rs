use std::collections::BTreeMap;
use std::ops::Range;
use std::path::PathBuf;

use law_model::{Edit, Finding, Fix, Safety};

use crate::fault::Fault;

/// One byte range of a file and what replaces it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Splice {
    pub(crate) bytes: Range<usize>,
    pub(crate) text: String,
}

/// The fixes taken from a set of findings: no two edits of a file overlap, the first by position wins.
#[derive(Debug, Default)]
pub(crate) struct Plan {
    pub(crate) files: BTreeMap<PathBuf, Vec<Splice>>,
    pub(crate) fixed: Vec<Finding>,
    pub(crate) skipped: Vec<Finding>,
    pub(crate) faults: Vec<Fault>,
}

type Taken = BTreeMap<PathBuf, BTreeMap<usize, usize>>;

impl Plan {
    fn conflicts(taken: Option<&BTreeMap<usize, usize>>, bytes: &Range<usize>) -> bool {
        taken.is_some_and(|taken| {
            let before = taken.range(..=bytes.start).next_back();
            let after = taken.range(bytes.start..).next();
            before.is_some_and(|(&start, &end)| start == bytes.start || end > bytes.start)
                || after.is_some_and(|(&start, _)| start == bytes.start || start < bytes.end)
        })
    }

    fn splices(fix: &Fix) -> Option<Vec<(PathBuf, Splice)>> {
        fix.edits
            .iter()
            .map(|edit| match edit {
                Edit::Replace { path, bytes, text } => Some((
                    path.clone(),
                    Splice {
                        bytes: bytes.clone(),
                        text: text.clone(),
                    },
                )),
                Edit::Create { .. } | Edit::Move { .. } => None,
            })
            .collect()
    }

    fn fits(taken: &Taken, splices: &[(PathBuf, Splice)]) -> bool {
        let mut local = Taken::new();
        splices.iter().all(|(path, splice)| {
            let free =
                !Self::conflicts(taken.get(path), &splice.bytes) && !Self::conflicts(local.get(path), &splice.bytes);
            local
                .entry(path.clone())
                .or_default()
                .insert(splice.bytes.start, splice.bytes.end);
            free
        })
    }

    fn take(mut self, taken: &mut Taken, finding: &Finding, fix: &Fix) -> Self {
        match Self::splices(fix) {
            None => self.faults.push(Fault::Unsupported {
                rule: finding.rule.clone(),
            }),
            Some(splices) if Self::fits(taken, &splices) => {
                for (path, splice) in splices {
                    taken
                        .entry(path.clone())
                        .or_default()
                        .insert(splice.bytes.start, splice.bytes.end);
                    self.files.entry(path).or_default().push(splice);
                }
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
        let mut taken = Taken::new();
        eligible.into_iter().fold(Self::default(), |plan, (finding, fix)| {
            plan.take(&mut taken, finding, fix)
        })
    }

    /// The fixed findings that touch none of the files that failed.
    pub(crate) fn fail(&mut self, failed: &[PathBuf]) {
        let touches = |finding: &Finding| {
            finding.fix.iter().flat_map(|fix| &fix.edits).any(|edit| match edit {
                Edit::Replace { path, .. } | Edit::Create { path, .. } => failed.contains(path),
                Edit::Move { from, to } => failed.contains(from) || failed.contains(to),
            })
        };
        self.fixed.retain(|finding| !touches(finding));
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
