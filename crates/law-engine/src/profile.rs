use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use law_model::{Aspect, Kind, Profile, ProfileSpec, Rule};

use crate::ancestry::{Ancestry, Line};
use crate::engine::{FileEngine, Sink};
use crate::outline::{Class, Outline};
use crate::scope::ScopeMatcher;
use crate::source::Source;

const PROTOCOL: [&str; 2] = ["typing.Protocol", "typing_extensions.Protocol"];
const ABC: &str = "abc.ABC";
const DOT: &str = ".";
const OR: &str = " or ";

/// A `profile` rule with its scope and the profiles of its kinds.
pub struct ProfileRule {
    rule: Rule,
    scope: ScopeMatcher,
    aspect: Aspect,
    profiles: Vec<Profile>,
}

/// The `profile` engine: each kind's entity classes judged for one aspect, their bases walked as `members` walks them.
pub struct ProfileEngine {
    rules: Vec<ProfileRule>,
    ancestry: Arc<Ancestry>,
}

/// What a class is known to hold and descend from, its own and through every base that resolved — the part of a
/// broken line that did resolve included: its methods, the names its line binds with a value (or by annotation alone
/// too, when the class declares a base: a protocol class, one naming `abc.ABC`, or one its line leaves a method abstract), the dotted
/// names written among the bases of every class of that line and of its ancestors; and whether its whole line
/// resolved, without which an absence proves nothing.
struct Known<'a> {
    methods: BTreeSet<&'a str>,
    attributes: BTreeSet<&'a str>,
    names: BTreeSet<String>,
    whole: bool,
}

impl<'a> Known<'a> {
    fn defined(class: &Class) -> HashMap<&str, bool> {
        class
            .bindings
            .iter()
            .filter(|binding| binding.valued)
            .map(|binding| (binding.name.as_str(), false))
            .chain(
                class
                    .methods
                    .iter()
                    .map(|method| (method.name.as_str(), method.abstracted())),
            )
            .collect()
    }

    fn abstracted(outline: &Outline, class: &Class, members: &[(&Outline, &Class)]) -> bool {
        let declared = class.bases.iter().any(|base| {
            let written = Ancestry::written(outline, base);
            written == ABC || PROTOCOL.contains(&written.as_str())
        });
        let mut seen = HashSet::new();
        declared
            || members
                .iter()
                .flat_map(|(_, member)| Self::defined(member))
                .filter(|(name, _)| seen.insert(*name))
                .any(|(_, pending)| pending)
    }

    fn new(outline: &'a Outline, class: &'a Class, line: &'a Line) -> Self {
        let ancestors = line
            .as_ref()
            .map_or_else(|missing| &*missing.resolved, |line| &*line.ancestors);
        let members: Vec<(&Outline, &Class)> = std::iter::once((outline, class))
            .chain(ancestors.iter().map(|ancestor| (&*ancestor.outline, &*ancestor.class)))
            .collect();
        let written = members
            .iter()
            .flat_map(|(outline, class)| class.bases.iter().map(|base| Ancestry::written(outline, base)));
        let dotted = ancestors.iter().filter_map(|ancestor| {
            ancestor
                .outline
                .module
                .as_ref()
                .map(|module| format!("{module}{DOT}{}", ancestor.class.name))
        });
        let missing = line.as_ref().err().map(|missing| missing.name.clone());
        let declared = Self::abstracted(outline, class, &members);
        Self {
            methods: members
                .iter()
                .flat_map(|(_, class)| class.methods.iter().map(|method| method.name.as_str()))
                .collect(),
            attributes: members
                .iter()
                .flat_map(|(_, class)| class.bindings.iter())
                .filter(|binding| binding.valued || declared)
                .map(|binding| binding.name.as_str())
                .collect(),
            names: written.chain(dotted).chain(missing).collect(),
            whole: line.is_ok(),
        }
    }
}

impl ProfileRule {
    fn homed<'a>(&'a self, path: &'a Path) -> impl Iterator<Item = &'a Profile> {
        self.profiles
            .iter()
            .filter(move |profile| profile.home.iter().any(|glob| glob.matches(path)))
    }

    fn entity(profile: &Profile, class: &Class) -> bool {
        profile
            .suffix
            .as_ref()
            .is_none_or(|suffix| class.name.ends_with(suffix.as_str()))
    }

    fn lineal(&self) -> bool {
        matches!(
            self.aspect,
            Aspect::Methods | Aspect::Attributes | Aspect::Line | Aspect::Never
        )
    }

    fn lacks(&self, path: &Path, class: &Class, wanted: &[String], held: &BTreeSet<&str>, sink: &mut Sink) {
        wanted
            .iter()
            .filter(|name| !held.contains(name.as_str()))
            .for_each(|name| {
                let detail = format!("{} lacks {name}", class.name);
                sink.report(&self.rule, path, class.span.clone(), Some(&detail), None);
            });
    }

    fn named(outline: &Outline, profile: &Profile, class: &Class) -> bool {
        class
            .bases
            .iter()
            .any(|base| profile.bases.contains(&Ancestry::written(outline, base)))
    }

    fn entities(
        &self,
        outline: &Outline,
        profile: &Profile,
        classes: &[(&Arc<Class>, Option<Known<'_>>)],
        sink: &mut Sink,
    ) {
        let path = outline.path.as_path();
        let suffix = profile.suffix.as_deref().unwrap_or_default();
        let mut entities = classes
            .iter()
            .filter(|(class, _)| Self::entity(profile, class))
            .peekable();
        match self.aspect {
            Aspect::Present if entities.peek().is_none() => {
                let detail = format!("no class ends in {suffix}");
                sink.report(&self.rule, path, Sink::origin(), Some(&detail), None);
            }
            Aspect::Present => {}
            Aspect::Surname => classes
                .iter()
                .filter(|(class, _)| !class.name.ends_with(suffix))
                .for_each(|(class, _)| {
                    let detail = format!("{} does not end in {suffix}", class.name);
                    sink.report(&self.rule, path, class.span.clone(), Some(&detail), None);
                }),
            Aspect::Bases => entities
                .filter(|(class, _)| !Self::named(outline, profile, class))
                .for_each(|(class, _)| {
                    let names = profile.bases.join(OR);
                    let detail = format!("{} does not name {names} among its bases", class.name);
                    sink.report(&self.rule, path, class.span.clone(), Some(&detail), None);
                }),
            Aspect::Methods | Aspect::Attributes | Aspect::Line | Aspect::Never => entities
                .filter_map(|(class, known)| known.as_ref().map(|known| (class, known)))
                .for_each(|(class, known)| self.judge(path, profile, class, known, sink)),
        }
    }

    fn judge(&self, path: &Path, profile: &Profile, class: &Class, known: &Known<'_>, sink: &mut Sink) {
        let span = class.span.clone();
        match self.aspect {
            Aspect::Methods if known.whole => self.lacks(path, class, &profile.methods, &known.methods, sink),
            Aspect::Attributes if known.whole => {
                self.lacks(path, class, &profile.attributes, &known.attributes, sink);
            }
            Aspect::Line if known.whole && !profile.line.iter().any(|name| known.names.contains(name)) => {
                let detail = format!("{} is not a {}", class.name, profile.line.join(OR));
                sink.report(&self.rule, path, span, Some(&detail), None);
            }
            Aspect::Never => {
                if let Some(name) = profile.never.iter().find(|name| known.names.contains(*name)) {
                    let detail = format!("{} is a {name}", class.name);
                    sink.report(&self.rule, path, span, Some(&detail), None);
                }
            }
            _ => {}
        }
    }

    fn admits(&self, path: &Path) -> bool {
        self.scope.admits(path) && self.homed(path).next().is_some()
    }

    /// A rule judging one aspect of the entity classes of its kinds, each kind's profile taken from those handed in.
    #[must_use]
    pub fn new(rule: &Rule, spec: &ProfileSpec, scope: ScopeMatcher, profiles: &BTreeMap<Kind, Profile>) -> Self {
        Self {
            rule: rule.clone(),
            scope,
            aspect: spec.aspect,
            profiles: spec
                .kinds
                .iter()
                .filter_map(|kind| profiles.get(kind))
                .cloned()
                .collect(),
        }
    }
}

impl ProfileEngine {
    /// The engine over its rules, its bases resolved by the run's resolver.
    #[must_use]
    pub(crate) fn new(rules: Vec<ProfileRule>, ancestry: Arc<Ancestry>) -> Self {
        Self { rules, ancestry }
    }
}

impl FileEngine for ProfileEngine {
    fn admits(&self, path: &Path) -> bool {
        self.rules.iter().any(|rule| rule.admits(path))
    }

    fn check(&self, source: &Source<'_>, sink: &mut Sink) {
        let path = source.path;
        let outline = Arc::new(Outline::new(path, &source.root.root()));
        let rules: Vec<&ProfileRule> = self.rules.iter().filter(|rule| rule.admits(path)).collect();
        let top: Vec<&Arc<Class>> = outline
            .every
            .iter()
            .filter(|class| {
                outline
                    .classes
                    .get(&class.name)
                    .is_some_and(|own| Arc::ptr_eq(own, class))
            })
            .collect();
        let lines: Vec<Option<Line>> = top
            .iter()
            .map(|class| {
                rules
                    .iter()
                    .any(|rule| rule.lineal() && rule.homed(path).any(|profile| ProfileRule::entity(profile, class)))
                    .then(|| self.ancestry.of(&outline, class))
            })
            .collect();
        let census = sink.census.profile.get_or_insert_default();
        lines.iter().flatten().for_each(|line| {
            census.judged += usize::from(line.is_ok());
            census.skipped += usize::from(line.is_err());
        });
        let classes: Vec<(&Arc<Class>, Option<Known<'_>>)> = top
            .iter()
            .zip(&lines)
            .map(|(class, line)| (*class, line.as_ref().map(|line| Known::new(&outline, class, line))))
            .collect();
        for rule in rules {
            for profile in rule.homed(path) {
                rule.entities(&outline, profile, &classes, sink);
            }
        }
    }
}
