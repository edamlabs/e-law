use std::collections::BTreeMap;
use std::fmt;
use std::ops::AddAssign;

/// Why a base did not resolve to a class a module writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Unresolved {
    /// The module exports the name through a module `__getattr__`, read only by running it.
    LazyExport,
    /// The module is compiled, with no stub beside it.
    CompiledModule,
    /// No root holds the module.
    NoSuchModule,
    /// The module binds the name to something no class statement writes, or the base is an expression.
    DynamicBase,
    /// The module may take the name from a star import that does not resolve.
    StarImport,
    /// The module binds the name nowhere.
    NoSuchName,
    /// The name comes through a relative import of a project module, which the project does not resolve.
    RelativeImport,
    /// The line of bases runs back into itself.
    Cycle,
}

/// How many entity classes the `profile` engine walked the line of: judged, every base resolved; skipped, one did not.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Profiled {
    /// The entity classes whose every base resolved.
    pub judged: usize,
    /// The entity classes with a base no module the run reads writes.
    pub skipped: usize,
}

/// How many classes the `members` engine judged, how many it skipped — neither lineage nor order judged — for a base
/// it could not resolve and why, how many of the judged stand on a builtin base other than `object`, and how many modules the resolver
/// read from the project's roots and from foreign ones.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Census {
    /// The classes whose every base resolved.
    pub judged: usize,
    /// The classes with a base no module the run reads writes.
    pub skipped: usize,
    /// The judged classes with a builtin base other than `object` somewhere in their line, its publics unknown.
    pub builtin: usize,
    /// The modules read from the project's roots.
    pub project: usize,
    /// The modules read from foreign roots.
    pub foreign: usize,
    /// Each base that did not resolve, by its dotted name: why, and how many classes it skipped.
    pub unresolved: BTreeMap<String, (Unresolved, usize)>,
    /// The entity classes the `profile` engine walked the line of; none when the run has no `profile` rule.
    pub profile: Option<Profiled>,
}

impl fmt::Display for Unresolved {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::LazyExport => "lazy export",
            Self::CompiledModule => "compiled module",
            Self::NoSuchModule => "no such module",
            Self::DynamicBase => "dynamic base",
            Self::StarImport => "star import",
            Self::NoSuchName => "no such name",
            Self::RelativeImport => "relative import",
            Self::Cycle => "cycle",
        })
    }
}

impl Census {
    /// A class skipped for a base, named, that did not resolve.
    pub fn skip(&mut self, name: &str, why: Unresolved) {
        self.skipped += 1;
        self.unresolved.entry(name.to_owned()).or_insert((why, 0)).1 += 1;
    }
}

impl AddAssign for Profiled {
    fn add_assign(&mut self, other: Self) {
        self.judged += other.judged;
        self.skipped += other.skipped;
    }
}

impl AddAssign for Census {
    fn add_assign(&mut self, other: Self) {
        self.judged += other.judged;
        self.skipped += other.skipped;
        self.builtin += other.builtin;
        self.project += other.project;
        self.foreign += other.foreign;
        other
            .unresolved
            .into_iter()
            .for_each(|(name, (why, count))| self.unresolved.entry(name).or_insert((why, 0)).1 += count);
        if let Some(other) = other.profile {
            *self.profile.get_or_insert_default() += other;
        }
    }
}
