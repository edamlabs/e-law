//! The external engines: ruff, ty and tach, commanded, their documents read, their diagnostics signed by the catalogue.

mod boundary;
mod diagnostic;
mod fault;
mod lines;
mod route;
mod ruff;
mod sarif;
mod tach;
mod tool;
mod ty;

pub use boundary::{
    TachCommand, TachDetails, TachDiagnostic, TachDocument, TachError, TachGlobal, TachKind, TachLocated, TachSeverity,
};
pub use diagnostic::{Applicability, RuffDiagnostic, RuffEdit, RuffFix, RuffLocation};
pub use fault::{Failure, Fault};
pub use lines::Lines;
pub use route::{RuffRoute, TachRoute, TyRoute};
pub use ruff::RuffEngine;
pub use sarif::{Sarif, SarifArtifact, SarifLocation, SarifMessage, SarifPhysical, SarifRegion, SarifResult, SarifRun};
pub use tach::TachEngine;
pub use tool::Tool;
pub use ty::TyEngine;
