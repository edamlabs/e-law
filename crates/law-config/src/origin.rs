use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::catalogue::{MANIFEST, PROFILES, RULES, YAML};
use crate::fault::Fault;

const NAMED: &str = ":";

/// A catalogue's files held in memory: each by its path from the catalogue's root, with its text, under the name its
/// faults are placed by (`eager:rules/lexis.yml`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bundle {
    /// The name.
    pub name: String,
    /// The files: `catalogue.toml`, `rules/**/*.yml`, `profiles/**/*.yml`.
    pub files: Vec<(PathBuf, String)>,
}

/// Where a catalogue is read from: a directory, or a bundle of files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// A catalogue directory.
    Dir(PathBuf),
    /// A catalogue's files in memory.
    Bundle(Bundle),
}

/// Where a catalogue's files are placed, for its faults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Place {
    Dir(PathBuf),
    Named(String),
}

/// A catalogue's files, from wherever they were read: the one set every catalogue is loaded from.
#[derive(Debug, Clone)]
pub(crate) struct Set {
    pub(crate) place: Place,
    pub(crate) files: Vec<(PathBuf, String)>,
}

impl Place {
    /// Where a file of the catalogue is, by its path from the catalogue's root.
    pub(crate) fn of(&self, relative: &Path) -> PathBuf {
        match self {
            Self::Dir(dir) => dir.join(relative),
            Self::Named(name) => PathBuf::from(format!("{name}{NAMED}{}", relative.display())),
        }
    }

    /// Where the catalogue itself is: its directory, or its name.
    pub(crate) fn root(&self) -> PathBuf {
        match self {
            Self::Dir(dir) => dir.clone(),
            Self::Named(name) => PathBuf::from(name),
        }
    }
}

impl Set {
    fn yaml(dir: &Path, faults: &mut Vec<Fault>) -> Vec<PathBuf> {
        let mut found = Vec::new();
        let mut pending = vec![dir.to_owned()];
        while let Some(current) = pending.pop() {
            match fs::read_dir(&current) {
                Ok(entries) => entries.for_each(|entry| match entry.map(|entry| entry.path()) {
                    Ok(path) if path.is_dir() => pending.push(path),
                    Ok(path) if path.extension().is_some_and(|extension| extension == YAML) => found.push(path),
                    Ok(_) => {}
                    Err(error) => faults.push(Fault::io(&current, &error)),
                }),
                Err(error) if current == dir && error.kind() == ErrorKind::NotFound => {}
                Err(error) => faults.push(Fault::io(&current, &error)),
            }
        }
        found
    }

    /// The files of a catalogue directory, and the faults met reading them.
    pub(crate) fn read(dir: &Path) -> (Self, Vec<Fault>) {
        let mut faults = Vec::new();
        let paths: Vec<PathBuf> = std::iter::once(dir.join(MANIFEST))
            .chain(
                [RULES, PROFILES]
                    .iter()
                    .flat_map(|sub| Self::yaml(&dir.join(sub), &mut faults)),
            )
            .collect();
        let files = paths
            .into_iter()
            .filter_map(|path| {
                let relative = path.strip_prefix(dir).map(Path::to_owned).ok()?;
                fs::read_to_string(&path)
                    .map(|text| (relative, text))
                    .map_err(|error| faults.push(Fault::io(&path, &error)))
                    .ok()
            })
            .collect();
        let set = Self {
            place: Place::Dir(dir.to_owned()),
            files,
        };
        (set, faults)
    }
}

impl From<Bundle> for Set {
    fn from(bundle: Bundle) -> Self {
        Self {
            place: Place::Named(bundle.name),
            files: bundle.files,
        }
    }
}
