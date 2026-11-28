use std::collections::BTreeMap;
use std::path::Path;

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use law_model::{Catalogue, Glob, Kind, Profile, Rule};

use crate::fault::Fault;

/// A rule's scope compiled once: its includes, its excludes, and the homes of its kind.
#[derive(Debug, Clone)]
pub struct ScopeMatcher {
    include: GlobSet,
    exclude: GlobSet,
    home: Option<GlobSet>,
}

impl ScopeMatcher {
    fn set<'a>(rule: &Rule, globs: impl IntoIterator<Item = &'a Glob>) -> Result<GlobSet, Fault> {
        let bad = |error: globset::Error| Fault::BadScope {
            rule: rule.id.clone(),
            message: error.to_string(),
        };
        globs
            .into_iter()
            .try_fold(GlobSetBuilder::new(), |mut builder, glob| {
                GlobBuilder::new(glob.as_str())
                    .literal_separator(true)
                    .build()
                    .map(|compiled| {
                        builder.add(compiled);
                        builder
                    })
            })
            .and_then(|builder| builder.build())
            .map_err(bad)
    }

    /// Every profile the catalogues declare, by its kind.
    #[must_use]
    pub fn profiles(catalogues: &[Catalogue]) -> BTreeMap<Kind, Profile> {
        catalogues
            .iter()
            .flat_map(|catalogue| &catalogue.profiles)
            .map(|profile| (profile.kind.clone(), profile.clone()))
            .collect()
    }

    /// The scope of a rule, its kind resolved through the profiles' homes.
    ///
    /// # Errors
    /// When a glob does not compile into a set.
    pub fn new(rule: &Rule, profiles: &BTreeMap<Kind, Profile>) -> Result<Self, Fault> {
        let home = rule
            .scope
            .kind
            .as_ref()
            .map(|kind| Self::set(rule, profiles.get(kind).into_iter().flat_map(|profile| &profile.home)))
            .transpose()?;
        Ok(Self {
            include: Self::set(rule, &rule.scope.include)?,
            exclude: Self::set(rule, &rule.scope.exclude)?,
            home,
        })
    }

    /// Whether the path is excluded, or outside the homes of the rule's kind.
    #[must_use]
    pub fn fences(&self, path: &Path) -> bool {
        self.exclude.is_match(path) || self.home.as_ref().is_some_and(|home| !home.is_match(path))
    }

    /// Whether the rule looks at the path: some include matches, no exclude does, and it sits in its kind's home.
    #[must_use]
    pub fn admits(&self, path: &Path) -> bool {
        self.include.is_match(path) && !self.fences(path)
    }
}
