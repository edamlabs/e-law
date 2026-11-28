use std::collections::HashSet;
use std::path::{Path, PathBuf};

use law_model::{Finding, Fix, Position, Rule, RuleId, Severity, Span, Tags};

use crate::census::Census;
use crate::digraph::Graph;
use crate::fault::Fault;
use crate::pragma::Pragma;
use crate::source::Source;

/// What engines write to: the findings, the inline pragmas, the faults of the paths they read, and the classes
/// judged.
#[derive(Debug, Default)]
pub struct Sink {
    /// The findings.
    pub findings: Vec<Finding>,
    /// The inline pragmas, licensed or reported once the run is whole.
    pub pragmas: Vec<Pragma>,
    /// The faults.
    pub faults: Vec<Fault>,
    /// The classes the `members` engine judged and skipped.
    pub census: Census,
}

/// What signs a finding: a catalogue's rule, or one of the engine's reserved rules.
pub trait Author {
    /// Its id.
    fn id(&self) -> &RuleId;
    /// What it says.
    fn message(&self) -> &str;
    /// How hard it blocks.
    fn severity(&self) -> Severity;
    /// Its labels.
    fn tags(&self) -> &Tags;
}

/// An engine that reads one parsed Python file at a time.
pub trait FileEngine: Sync {
    /// Whether any of its rules looks at the path.
    fn admits(&self, path: &Path) -> bool;

    /// The findings of its rules in the file.
    fn check(&self, source: &Source<'_>, sink: &mut Sink);
}

/// An engine that sees the whole run at once: an external tool over the paths of the run.
pub trait BatchEngine: Sync {
    /// Its name.
    fn name(&self) -> &str;

    /// The version of what it runs, as that reports it.
    fn version(&self) -> &str;

    /// The rules it checks.
    fn rules(&self) -> &[RuleId];

    /// The findings of its rules over the paths, from the project root.
    fn check(&self, root: &Path, paths: &[PathBuf], sink: &mut Sink);
}

/// An engine that sees every module of the project at once, once the file pass is done: their import graph.
pub trait ProjectEngine: Sync {
    /// The findings of its rules over the whole graph; given the paths the run was asked for, a finding that spans
    /// modules is anchored in one of them.
    fn check(&self, graph: &Graph, asked: Option<&HashSet<&Path>>, sink: &mut Sink);
}

/// A batch engine that proves one of its rules alone against one case: a text at a path.
pub trait CaseEngine: BatchEngine {
    /// The findings of the rule alone in the text, as if it were the file at the path.
    fn case(&self, rule: &RuleId, path: &Path, text: &str, sink: &mut Sink);
}

impl Author for Rule {
    fn id(&self) -> &RuleId {
        &self.id
    }

    fn message(&self) -> &str {
        self.message.as_str()
    }

    fn severity(&self) -> Severity {
        self.severity
    }

    fn tags(&self) -> &Tags {
        &self.tags
    }
}

impl Sink {
    /// The span of a finding about a whole file or directory: its first line.
    #[must_use]
    pub fn origin() -> Span {
        let start = Position { line: 1, column: 1 };
        Span {
            start,
            end: start,
            bytes: 0..0,
        }
    }

    /// A finding of a rule at a path, its message followed by what was found, with the edits that resolve it.
    pub fn report(&mut self, rule: &impl Author, path: &Path, span: Span, detail: Option<&str>, fix: Option<Fix>) {
        let message = detail.map_or_else(
            || rule.message().to_owned(),
            |detail| format!("{}: {detail}", rule.message()),
        );
        self.findings.push(Finding {
            rule: rule.id().clone(),
            path: path.to_owned(),
            span,
            message,
            severity: rule.severity(),
            tags: rule.tags().clone(),
            fix,
        });
    }

    /// Another sink's findings, pragmas, faults and census, taken in.
    pub fn absorb(&mut self, other: Self) {
        self.findings.extend(other.findings);
        self.pragmas.extend(other.pragmas);
        self.faults.extend(other.faults);
        self.census += other.census;
    }
}
