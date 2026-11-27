//! Fixes: the edits of findings applied, on disk or in memory.

mod apply;
mod fault;
mod plan;

pub use apply::{Applied, apply, patch};
pub use fault::Fault;
