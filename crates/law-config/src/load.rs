use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::fs;
use std::path::{Path, PathBuf};

use law_model::{
    Catalogue, Check, Exception, Former, Kind, Measure, MetricSpec, Profile, ProfileSpec, RuffSpec, Rule, RuleId,
    Snippet, TachSpec, TySpec,
};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::catalogue::{MANIFEST, Manifest, PROFILES, RULES, YAML};
use crate::config::{Config, LAW, PYPROJECT, Source};
use crate::exceptions::Exceptions;
use crate::fault::{Fault, Lacking};
use crate::origin::{Origin, Set};

const MISSING: &str = "missing: a catalogue declares itself in its catalogue.toml";

/// How the composition root resolves a built-in catalogue's name to where it is read from.
pub type Builtins<'a> = &'a dyn Fn(&str) -> Option<Origin>;

/// A configuration and the project root every path in it is relative to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    /// The project root.
    pub root: PathBuf,
    /// The file the configuration came from.
    pub path: PathBuf,
    /// The configuration.
    pub config: Config,
}

struct Layer {
    dir: PathBuf,
    catalogue: Catalogue,
    files: Vec<PathBuf>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entries {
    #[serde(default)]
    exception: Vec<toml::Spanned<toml::Value>>,
}

/// Everything a run needs from the configuration: the project, its catalogues in order, its exceptions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    /// The project.
    pub project: Project,
    /// The catalogues, in the order they layer.
    pub catalogues: Vec<Catalogue>,
    /// The declared exceptions.
    pub exceptions: Vec<Exception>,
}

impl Project {
    /// The project a configuration file declares, rooted at its directory; none for a `pyproject.toml` without `[tool.law]`.
    ///
    /// # Errors
    /// When the file cannot be read or does not decode.
    pub fn read(path: &Path) -> Result<Option<Self>, Fault> {
        let text = fs::read_to_string(path).map_err(|error| Fault::io(path, &error))?;
        Ok(Config::parse(&text, path)?.map(|config| Self {
            root: path.parent().map(Path::to_owned).unwrap_or_default(),
            path: path.to_owned(),
            config,
        }))
    }

    fn found(dir: &Path) -> Option<Result<Self, Vec<Fault>>> {
        let law = dir.join(LAW);
        let pyproject = dir.join(PYPROJECT);
        let read = |path: &Path| {
            path.is_file()
                .then(|| Self::read(path))
                .transpose()
                .map(Option::flatten)
        };
        match (read(&law), read(&pyproject)) {
            (Ok(Some(_)), Ok(Some(_))) => Some(Err(vec![Fault::TwoConfigs { law, pyproject }])),
            (Ok(Some(project)), Ok(None)) | (Ok(None), Ok(Some(project))) => Some(Ok(project)),
            (Ok(None), Ok(None)) => None,
            (law, pyproject) => Some(Err([law.err(), pyproject.err()].into_iter().flatten().collect())),
        }
    }
}

/// The project found from a directory upward: the first holding `law.toml` or a `pyproject.toml` with `[tool.law]`.
///
/// # Errors
/// When the configuration found cannot be read or decoded, or one directory holds two.
pub fn discover(start: &Path) -> Result<Option<Project>, Vec<Fault>> {
    start.ancestors().find_map(Project::found).transpose()
}

fn yaml<T: DeserializeOwned>(path: &Path, text: &str) -> Result<Vec<T>, Vec<Fault>> {
    serde_saphyr::from_str(text).map_err(|error| {
        let first = Fault::yaml(path, &error);
        serde_saphyr::from_str::<Vec<serde_saphyr::Spanned<Value>>>(text).map_or_else(
            |_| vec![first.clone()],
            |items| {
                let each = items.into_iter().filter_map(|item| {
                    let line = Some(item.referenced.line()).filter(|&line| line > 0);
                    serde_json::from_value::<T>(item.value)
                        .err()
                        .map(|error| Fault::decoded(path, line, &error.to_string()))
                });
                Fault::every(first.clone(), each)
            },
        )
    })
}

fn lists<T: DeserializeOwned>(set: &Set, under: &str, faults: &mut Vec<Fault>) -> Vec<(PathBuf, T)> {
    set.files
        .iter()
        .filter(|(relative, _)| {
            relative.starts_with(under) && relative.extension().is_some_and(|extension| extension == YAML)
        })
        .filter_map(|(relative, text)| {
            let path = set.place.of(relative);
            match yaml::<T>(&path, text) {
                Ok(items) => Some(
                    items
                        .into_iter()
                        .map(move |item| (path.clone(), item))
                        .collect::<Vec<_>>(),
                ),
                Err(errors) => {
                    faults.extend(errors);
                    None
                }
            }
        })
        .flatten()
        .collect()
}

fn manifest(set: &Set, faults: &mut Vec<Fault>) -> Option<Manifest> {
    let path = set.place.of(Path::new(MANIFEST));
    let found = set.files.iter().find(|(relative, _)| relative == Path::new(MANIFEST));
    let Some((_, text)) = found else {
        let read = faults
            .iter()
            .any(|fault| matches!(fault, Fault::Io { path: unread, .. } if *unread == path));
        if !read {
            faults.push(Fault::Io {
                path,
                message: MISSING.to_owned(),
            });
        }
        return None;
    };
    toml::from_str::<Manifest>(text)
        .map_err(|error| faults.push(Fault::toml(&path, text, &error)))
        .ok()
}

fn duplicates<K: Ord + Clone, V: Clone>(
    entries: impl Iterator<Item = (K, V)>,
    fault: impl Fn(K, V, V) -> Fault,
) -> Vec<Fault> {
    let mut first: BTreeMap<K, V> = BTreeMap::new();
    entries
        .filter_map(|(key, path)| match first.entry(key) {
            Entry::Occupied(seen) => Some(fault(seen.key().clone(), seen.get().clone(), path)),
            Entry::Vacant(slot) => {
                slot.insert(path);
                None
            }
        })
        .collect()
}

fn rule_twice(id: RuleId, first: PathBuf, second: PathBuf) -> Fault {
    Fault::DuplicateRule { id, first, second }
}

fn former_twice(id: Former, first: (RuleId, PathBuf), second: (RuleId, PathBuf)) -> Fault {
    Fault::DuplicateFormer {
        id,
        first: first.0,
        second: second.0,
        path: second.1,
    }
}

fn profile_twice(kind: Kind, first: PathBuf, second: PathBuf) -> Fault {
    Fault::DuplicateProfile { kind, first, second }
}

fn taken(rule: &Rule) -> Option<String> {
    match &rule.check {
        Check::Ruff(spec @ RuffSpec { message: None, .. }) => Some(format!(
            "ruff `{}`{}",
            spec.code,
            spec.overrides()
                .iter()
                .fold(String::new(), |key, over| key + " with " + over)
        )),
        Check::Ty(TySpec { rule: Some(name) }) => Some(format!("ty `{name}`")),
        Check::Ty(TySpec { rule: None }) => Some("ty".to_owned()),
        Check::Tach(TachSpec { code: Some(code) }) => Some(format!("tach `{code}`")),
        Check::Tach(TachSpec { code: None }) => Some("tach".to_owned()),
        _ => None,
    }
}

fn filed(path: &Path, rule: &Rule) -> Option<Fault> {
    let cases = &rule.cases;
    let filed = cases
        .valid
        .iter()
        .chain(&cases.invalid)
        .any(|case| matches!(case.snippet, Snippet::Files(_)));
    (filed && !rule.check.filed()).then(|| Fault::FilesCase {
        path: path.to_owned(),
        rule: rule.id.clone(),
        engine: rule.check.name(),
    })
}

fn coded(path: &Path, rule: &Rule) -> Option<Fault> {
    let cases = &rule.cases;
    let coded = cases
        .valid
        .iter()
        .chain(&cases.invalid)
        .any(|case| matches!(case.snippet, Snippet::Code(_)));
    (coded && rule.check.filed()).then(|| Fault::CodeCase {
        path: path.to_owned(),
        rule: rule.id.clone(),
        engine: rule.check.name(),
    })
}

fn indistinct(key: String, first: (RuleId, PathBuf), second: (RuleId, PathBuf)) -> Fault {
    Fault::Indistinct {
        key,
        first: first.0,
        second: second.0,
        path: second.1,
    }
}

fn read(mut set: Set, mut faults: Vec<Fault>) -> Result<(Catalogue, Vec<PathBuf>), Vec<Fault>> {
    set.files.sort_by(|(a, _), (b, _)| a.cmp(b));
    let manifest = manifest(&set, &mut faults);
    let rules: Vec<(PathBuf, Rule)> = lists(&set, RULES, &mut faults);
    let profiles: Vec<(PathBuf, Profile)> = lists(&set, PROFILES, &mut faults);
    faults.extend(duplicates(
        rules.iter().map(|(path, rule)| (rule.id.clone(), path.clone())),
        rule_twice,
    ));
    faults.extend(duplicates(
        profiles
            .iter()
            .map(|(path, profile)| (profile.kind.clone(), path.clone())),
        profile_twice,
    ));
    faults.extend(duplicates(
        rules.iter().flat_map(|(path, rule)| {
            rule.formerly
                .iter()
                .map(|former| (former.clone(), (rule.id.clone(), path.clone())))
        }),
        former_twice,
    ));
    faults.extend(rules.iter().filter_map(|(path, rule)| filed(path, rule)));
    faults.extend(rules.iter().filter_map(|(path, rule)| coded(path, rule)));
    match manifest {
        Some(Manifest {
            name, version, prose, ..
        }) if faults.is_empty() => {
            let (files, rules) = rules.into_iter().unzip();
            Ok((
                Catalogue {
                    name,
                    version,
                    rules,
                    profiles: profiles.into_iter().map(|(_, profile)| profile).collect(),
                    prose,
                },
                files,
            ))
        }
        _ => Err(faults),
    }
}

fn origin(origin: Origin) -> (Set, Vec<Fault>) {
    match origin {
        Origin::Dir(dir) => Set::read(&dir),
        Origin::Bundle(bundle) => (Set::from(bundle), Vec::new()),
    }
}

/// The catalogue a directory holds: `catalogue.toml`, `rules/**/*.yml` and `profiles/**/*.yml`.
///
/// # Errors
/// Every fault found: unreadable or undecodable files, refused values, a rule id or a profile kind declared twice.
pub fn catalogue(dir: &Path) -> Result<Catalogue, Vec<Fault>> {
    let (set, faults) = Set::read(dir);
    read(set, faults).map(|(catalogue, _)| catalogue)
}

fn exceptions(project: &Project) -> Result<Vec<Exception>, Vec<Fault>> {
    project.config.exceptions.as_ref().map_or_else(
        || Ok(Vec::new()),
        |relative| {
            let path = project.root.join(relative);
            let text = fs::read_to_string(&path).map_err(|error| vec![Fault::io(&path, &error)])?;
            toml::from_str::<Exceptions>(&text)
                .map(|file| file.exception)
                .map_err(|error| {
                    let first = Fault::toml(&path, &text, &error);
                    toml::from_str::<Entries>(&text).map_or_else(
                        |_| vec![first.clone()],
                        |entries| {
                            let each = entries.exception.into_iter().filter_map(|entry| {
                                let line = Fault::line(&text, entry.span().start);
                                entry
                                    .into_inner()
                                    .try_into::<Exception>()
                                    .err()
                                    .map(|error| Fault::decoded(&path, line, error.message()))
                            });
                            Fault::every(first.clone(), each)
                        },
                    )
                })
        },
    )
}

fn resolve(project: &Project, source: &Source, builtins: Builtins<'_>) -> Result<Origin, Fault> {
    match source {
        Source::Dir(dir) => Ok(Origin::Dir(project.root.join(dir))),
        Source::Builtin(name) => builtins(name).ok_or_else(|| Fault::UnknownCatalogue {
            path: project.path.clone(),
            name: name.clone(),
        }),
    }
}

fn measured(rule: &Rule, file: &Path, declared: &BTreeMap<&Kind, &Profile>) -> Vec<Fault> {
    let Check::Metric(MetricSpec {
        kinds, measure, over, ..
    }) = &rule.check
    else {
        return Vec::new();
    };
    kinds
        .iter()
        .flat_map(|kind| {
            let unknown = declared.get(kind).is_none().then(|| Fault::UnknownKind {
                path: file.to_owned(),
                kind: kind.clone(),
            });
            let profile = declared.get(kind);
            let suffix = profile
                .filter(|profile| *measure == Measure::Companions && profile.suffix.is_none())
                .map(|_| Fault::UnfedMeasure {
                    path: file.to_owned(),
                    rule: rule.id.clone(),
                    kind: kind.clone(),
                    measure: *measure,
                    lacking: Lacking::Suffix,
                });
            let threshold = profile
                .filter(|profile| !profile.thresholds.contains_key(over))
                .map(|_| Fault::UnfedMeasure {
                    path: file.to_owned(),
                    rule: rule.id.clone(),
                    kind: kind.clone(),
                    measure: *measure,
                    lacking: Lacking::Threshold(over.clone()),
                });
            unknown.into_iter().chain(suffix).chain(threshold)
        })
        .collect()
}

fn judged<'a>(rule: &'a Rule, file: &'a Path, declared: &'a BTreeMap<&Kind, &Profile>) -> Vec<Fault> {
    let Check::Profile(ProfileSpec { kinds, aspect }) = &rule.check else {
        return measured(rule, file, declared);
    };
    kinds
        .iter()
        .filter_map(|kind| match declared.get(kind) {
            None => Some(Fault::UnknownKind {
                path: file.to_owned(),
                kind: kind.clone(),
            }),
            Some(profile) if !profile.feeds(*aspect) => Some(Fault::UnfedAspect {
                path: file.to_owned(),
                rule: rule.id.clone(),
                kind: kind.clone(),
                aspect: *aspect,
            }),
            Some(_) => None,
        })
        .collect()
}

fn kinds(project: &Project, layered: &[Layer]) -> Vec<Fault> {
    let declared: BTreeMap<&Kind, &Profile> = layered
        .iter()
        .flat_map(|layer| &layer.catalogue.profiles)
        .map(|profile| (&profile.kind, profile))
        .collect();
    let named = layered.iter().flat_map(|layer| {
        layer
            .catalogue
            .rules
            .iter()
            .zip(&layer.files)
            .filter_map(|(rule, file)| rule.scope.kind.as_ref().map(|kind| (kind, file)))
            .filter(|(kind, _)| !declared.contains_key(kind))
            .map(|(kind, file)| Fault::UnknownKind {
                path: file.clone(),
                kind: kind.clone(),
            })
    });
    let profiled = layered.iter().flat_map(|layer| {
        layer
            .catalogue
            .rules
            .iter()
            .zip(&layer.files)
            .flat_map(|(rule, file)| judged(rule, file, &declared))
    });
    let overridden = project.config.profiles.iter().flat_map(|(kind, overrides)| {
        let profile = declared.get(kind);
        let unknown = profile.is_none().then(|| Fault::UnknownKind {
            path: project.path.clone(),
            kind: kind.clone(),
        });
        let thresholds = overrides
            .thresholds
            .keys()
            .filter(move |name| profile.is_none_or(|profile| !profile.thresholds.contains_key(*name)))
            .map(|name| Fault::UnknownThreshold {
                path: project.path.clone(),
                kind: kind.clone(),
                name: name.clone(),
            });
        unknown.into_iter().chain(thresholds)
    });
    named.chain(profiled).chain(overridden).collect()
}

/// A project's catalogues layered in order, checked against each other, and its exceptions.
///
/// # Errors
/// Every fault found across every catalogue, the layering and the exceptions file.
pub fn load(project: &Project, builtins: Builtins<'_>) -> Result<Loaded, Vec<Fault>> {
    let mut faults = Vec::new();
    let layered: Vec<Layer> = project
        .config
        .catalogues
        .iter()
        .filter_map(|source| {
            resolve(project, source, builtins)
                .map_err(|fault| vec![fault])
                .and_then(|origin| {
                    let (set, faults) = self::origin(origin);
                    let dir = set.place.root();
                    read(set, faults).map(|(catalogue, files)| Layer { dir, catalogue, files })
                })
                .map_err(|errors| faults.extend(errors))
                .ok()
        })
        .collect();
    faults.extend(duplicates(
        layered.iter().flat_map(|layer| {
            layer
                .catalogue
                .rules
                .iter()
                .map(|rule| (rule.id.clone(), layer.dir.clone()))
        }),
        rule_twice,
    ));
    faults.extend(duplicates(
        layered.iter().flat_map(|layer| {
            layer.catalogue.rules.iter().flat_map(|rule| {
                rule.formerly
                    .iter()
                    .map(|former| (former.clone(), (rule.id.clone(), layer.dir.clone())))
            })
        }),
        former_twice,
    ));
    faults.extend(duplicates(
        layered.iter().flat_map(|layer| {
            layer
                .catalogue
                .profiles
                .iter()
                .map(|profile| (profile.kind.clone(), layer.dir.clone()))
        }),
        profile_twice,
    ));
    faults.extend(duplicates(
        layered.iter().flat_map(|layer| {
            layer
                .catalogue
                .rules
                .iter()
                .zip(&layer.files)
                .filter_map(|(rule, file)| taken(rule).map(|key| (key, (rule.id.clone(), file.clone()))))
        }),
        indistinct,
    ));
    faults.extend(kinds(project, &layered));
    let exceptions = exceptions(project).map_err(|errors| faults.extend(errors)).ok();
    match exceptions {
        Some(exceptions) if faults.is_empty() => Ok(Loaded {
            project: project.clone(),
            catalogues: layered.into_iter().map(|layer| layer.catalogue).collect(),
            exceptions,
        }),
        _ => Err(faults),
    }
}
