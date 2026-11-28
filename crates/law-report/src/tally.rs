/// What a report's text closes with besides its findings: the fixes applied, and the faults that left it incomplete.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tally {
    /// The findings fixed, when fixes were applied.
    pub fixed: Option<usize>,
    /// The faults of the run.
    pub faults: usize,
}
