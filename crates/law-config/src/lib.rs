//! Configuration: loading, layering and selection.

mod catalogue;
mod config;
mod exceptions;
mod fault;
mod load;
mod origin;
mod schema;
mod select;

pub use catalogue::Manifest;
pub use config::{Config, Overrides, Source};
pub use exceptions::Exceptions;
pub use fault::Fault;
pub use load::{Builtins, Loaded, Project, catalogue, discover, load};
pub use origin::{Bundle, Origin};
pub use schema::{Schemas, schema};
pub use select::Selection;
