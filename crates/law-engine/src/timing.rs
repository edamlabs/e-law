use std::time::Duration;

/// How long one phase of a run took: the walk, the parse, or an engine by its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timing {
    /// The phase.
    pub phase: String,
    /// What it took.
    pub elapsed: Duration,
}

impl Timing {
    /// The timing of a phase.
    #[must_use]
    pub fn new(phase: &str, elapsed: Duration) -> Self {
        Self {
            phase: phase.to_owned(),
            elapsed,
        }
    }
}
