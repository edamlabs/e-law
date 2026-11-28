use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use jiff::civil::Date;
use law_model::{Edit, Exception, Finding, Fix, Former, Reach, RuleId, Safety, Tags};

use crate::engine::Sink;
use crate::fault::Fault;
use crate::pragma::Pragma;
use crate::reserved::ReservedRule;

const BARE: &str = "names no rule";

/// The declared exceptions of a run, split at load into those that hold and those past their date.
#[derive(Debug, Clone)]
pub struct ExceptionLedger {
    file: PathBuf,
    live: Vec<Exception>,
    expired: Vec<Exception>,
}

impl ExceptionLedger {
    fn silences(exception: &Exception, finding: &Finding) -> bool {
        exception.reach() == Reach::File
            && exception.rule().chooses(&finding.rule, &finding.tags)
            && exception.path().is_some_and(|glob| glob.matches(&finding.path))
    }

    fn licenses(exception: &Exception, id: &RuleId, tags: &Tags, path: &Path) -> bool {
        exception.reach() == Reach::Pragma
            && exception.rule().chooses(id, tags)
            && exception.path().is_some_and(|glob| glob.matches(path))
    }

    fn detail(exception: &Exception) -> String {
        exception.path().map_or_else(
            || format!("`{}`", exception.rule()),
            |path| format!("`{}` at {path}", exception.rule()),
        )
    }

    fn mark(&self, used: &mut [bool], chosen: impl Fn(&Exception) -> bool) -> bool {
        self.live
            .iter()
            .zip(used.iter_mut())
            .filter(|(exception, _)| chosen(exception))
            .fold(false, |_, (_, flag)| {
                *flag = true;
                true
            })
    }

    fn respelled(pragma: &Pragma, old: &str, new: &RuleId) -> (String, Option<Fix>) {
        let fix = pragma.locate(old).map(|bytes| Fix {
            safety: Safety::Safe,
            edits: vec![Edit::Replace {
                path: pragma.path.clone(),
                bytes,
                text: new.to_string(),
            }],
        });
        (format!("{old} is now {new}"), fix)
    }

    fn verdict(
        &self,
        pragma: &Pragma,
        id: &str,
        known: &BTreeMap<RuleId, Tags>,
        former: &BTreeMap<Former, RuleId>,
        used: &mut [bool],
    ) -> Option<(String, Option<Fix>)> {
        let rule = id.parse::<RuleId>().ok();
        let renamed = id.parse::<Former>().ok().and_then(|old| former.get(&old));
        match (
            renamed,
            rule.as_ref().and_then(|rule| known.get(rule).map(|tags| (rule, tags))),
        ) {
            (Some(new), _) => Some(Self::respelled(pragma, id, new)),
            (None, None) => Some((format!("{id} is not a rule"), None)),
            (None, Some((rule, tags))) => (!self
                .mark(used, |exception| Self::licenses(exception, rule, tags, &pragma.path)))
            .then(|| (format!("{id} has no registered exception"), None)),
        }
    }

    /// The exceptions of a file, from the project root, as of a day.
    ///
    /// # Errors
    /// Every exception naming a `symbol`, which this cut does not resolve.
    pub fn new(file: &Path, exceptions: &[Exception], today: Date) -> Result<Self, Vec<Fault>> {
        let symbols: Vec<Fault> = exceptions
            .iter()
            .filter(|exception| exception.symbol().is_some())
            .map(|exception| Fault::SymbolException {
                rule: exception.rule().clone(),
            })
            .collect();
        let (expired, live) = exceptions
            .iter()
            .cloned()
            .partition(|exception| exception.expires().is_some_and(|day| day < today));
        if symbols.is_empty() {
            Ok(Self {
                file: file.to_owned(),
                live,
                expired,
            })
        } else {
            Err(symbols)
        }
    }

    /// The flags, one per live exception, of which held something; all down.
    #[must_use]
    pub fn unmarked(&self) -> Vec<bool> {
        vec![false; self.live.len()]
    }

    /// The findings no live `file` exception silences; each one that silenced marked in `used`.
    #[must_use]
    pub fn apply(&self, findings: Vec<Finding>, used: &mut [bool]) -> Vec<Finding> {
        findings
            .into_iter()
            .filter(|finding| !self.mark(used, |exception| Self::silences(exception, finding)))
            .collect()
    }

    /// One finding, signed by the reserved rule, per id a pragma names that no live `pragma` exception licenses
    /// there, or that no rule has — a former id of a rule, with a safe fix respelling it, in place of both; for a pragma any one id licenses, one for its first id when none does — and one
    /// per pragma naming no id; each licence that held marked in `used`.
    pub fn license(
        &self,
        pragmas: &[Pragma],
        known: &BTreeMap<RuleId, Tags>,
        former: &BTreeMap<Former, RuleId>,
        rule: &ReservedRule,
        used: &mut [bool],
        sink: &mut Sink,
    ) {
        let verdicts: Vec<(&Pragma, (String, Option<Fix>))> = pragmas
            .iter()
            .flat_map(|pragma| {
                let details: Vec<(String, Option<Fix>)> = if pragma.ids.is_empty() {
                    vec![(BARE.to_owned(), None)]
                } else {
                    let verdicts: Vec<Option<(String, Option<Fix>)>> = pragma
                        .ids
                        .iter()
                        .map(|id| self.verdict(pragma, id, known, former, used))
                        .collect();
                    match (pragma.any, verdicts.iter().any(Option::is_none)) {
                        (true, true) => Vec::new(),
                        (true, false) => verdicts.into_iter().flatten().take(1).collect(),
                        (false, _) => verdicts.into_iter().flatten().collect(),
                    }
                };
                details.into_iter().map(move |detail| (pragma, detail))
            })
            .collect();
        for (pragma, (detail, fix)) in verdicts {
            sink.report(rule, &pragma.path, pragma.span.clone(), Some(&detail), fix);
        }
    }

    /// One finding per expired exception, signed by the reserved rule, at the exceptions file.
    pub fn expired(&self, rule: &ReservedRule, sink: &mut Sink) {
        self.expired.iter().for_each(|exception| {
            let detail = Self::detail(exception);
            sink.report(rule, &self.file, Sink::origin(), Some(&detail), None);
        });
    }

    /// One finding per live exception that held nothing, signed by the reserved rule, at the exceptions file; a `file`
    /// exception is not judged only when it chooses a loaded rule (the reserved ones among them) and none of the
    /// admitted ones, since its rules did not run: one choosing no loaded rule at all is always unused.
    pub fn unused(
        &self,
        rule: &ReservedRule,
        used: &[bool],
        known: &BTreeMap<RuleId, Tags>,
        admitted: &[(RuleId, Tags)],
        sink: &mut Sink,
    ) {
        let chooses = |exception: &Exception, id: &RuleId, tags: &Tags| exception.rule().chooses(id, tags);
        let judged = |exception: &Exception| {
            exception.reach() == Reach::Pragma
                || admitted.iter().any(|(id, tags)| chooses(exception, id, tags))
                || !known.iter().any(|(id, tags)| chooses(exception, id, tags))
        };
        self.live
            .iter()
            .zip(used)
            .filter(|(exception, used)| !**used && judged(exception))
            .for_each(|(exception, _)| {
                let detail = Self::detail(exception);
                sink.report(rule, &self.file, Sink::origin(), Some(&detail), None);
            });
    }
}
