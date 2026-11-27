use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::graph::GraphSpec;
use crate::members::MembersSpec;
use crate::parity::ParitySpec;
use crate::pattern::PatternSpec;
use crate::profile::ProfileSpec;
use crate::ruff::RuffSpec;
use crate::shape::ShapeSpec;
use crate::tach::TachSpec;
use crate::tree::TreeSpec;
use crate::ty::TySpec;

/// The engine a rule runs on, with that engine's spec.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "engine", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Check {
    /// What directories hold.
    Tree(TreeSpec),
    /// What a module's top level holds.
    Shape(ShapeSpec),
    /// An ast-grep rule over the syntax tree.
    Pattern(PatternSpec),
    /// A ruff rule, by its code.
    Ruff(RuffSpec),
    /// A ty rule, by its name, or every ty diagnostic.
    Ty(TySpec),
    /// What a class's methods hold: the lineage of its privates, their order.
    Members(MembersSpec),
    /// A tach diagnostic, by its kind, or every tach diagnostic.
    Tach(TachSpec),
    /// What the entity classes of a profile's kinds hold, how they are named, what they descend from.
    Profile(ProfileSpec),
    /// What the import graph of the project's modules holds: its cycles, its orphans.
    Graph(GraphSpec),
    /// What the names a file's classes declare stand for: the modules beside it, or the files of a glob.
    Parity(ParitySpec),
}

impl Check {
    /// The name of its engine, as the `engine` tag writes it.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Tree(_) => "tree",
            Self::Shape(_) => "shape",
            Self::Pattern(_) => "pattern",
            Self::Ruff(_) => "ruff",
            Self::Ty(_) => "ty",
            Self::Members(_) => "members",
            Self::Tach(_) => "tach",
            Self::Profile(_) => "profile",
            Self::Graph(_) => "graph",
            Self::Parity(_) => "parity",
        }
    }

    /// Whether it is proven by `files` cases alone: a `code` case proves nothing of it.
    #[must_use]
    pub fn filed(&self) -> bool {
        matches!(self, Self::Graph(_) | Self::Parity(_))
    }
}
