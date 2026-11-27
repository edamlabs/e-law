//! Reports: findings in text, JSON, SARIF and Markdown; trials, rules and coverage in text and JSON.

mod count;
mod coverage;
mod format;
mod markdown;
mod rank;
mod rule;
mod sarif;
mod tally;
mod trials;
mod write;

pub use coverage::coverage;
pub use format::{Format, Output, Table};
pub use rule::{rule, rules};
pub use tally::Tally;
pub use trials::trials;
pub use write::write;
