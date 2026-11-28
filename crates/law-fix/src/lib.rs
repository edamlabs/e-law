//! Fixes: the edits of findings applied, on disk or in memory.

mod apply;
mod fault;
mod ground;
mod plan;

pub use apply::{Applied, apply, patch, project};
pub use fault::Fault;
