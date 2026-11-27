use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

const VENV: &str = ".venv";
const CONFIG: &str = "pyvenv.cfg";
const HOME: &str = "home";
const VERSION_INFO: &str = "version_info";
const VERSION: &str = "version";
const PYTHON: &str = "python";
const SITE: &str = "site-packages";
const DYNLOAD: &str = "lib-dynload";
const LIBS: [&str; 2] = ["lib", "lib64"];
const LIB: &str = "lib";
const EQUALS: char = '=';
const DOT: char = '.';

/// The interpreter environment of a project, read off its `.venv` and never run: the roots its foreign modules live
/// under, in the interpreter's order — the standard library, its compiled modules, then the site-packages; none
/// without a `.venv`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Environment {
    /// The virtual environment, when the project has one.
    pub venv: Option<PathBuf>,
    /// The standard library, when the venv's `pyvenv.cfg` names its interpreter and version.
    pub stdlib: Option<PathBuf>,
    /// The foreign roots, searched in order after the project's own.
    pub roots: Vec<PathBuf>,
}

impl Environment {
    fn key<'a>(config: &'a str, key: &str) -> Option<&'a str> {
        config.lines().find_map(|line| {
            let (name, value) = line.split_once(EQUALS)?;
            (name.trim() == key).then(|| value.trim())
        })
    }

    fn short(version: &str) -> Option<String> {
        let mut parts = version.split(DOT);
        let (major, minor) = (parts.next()?, parts.next()?);
        Some(format!("{major}{DOT}{minor}"))
    }

    fn sites(venv: &Path) -> Vec<PathBuf> {
        let mut sites: Vec<PathBuf> = LIBS
            .iter()
            .filter_map(|lib| fs::read_dir(venv.join(lib)).ok())
            .flatten()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().starts_with(PYTHON))
            .map(|entry| entry.path().join(SITE))
            .filter_map(|site| fs::canonicalize(site).ok())
            .collect();
        sites.sort();
        sites.dedup();
        sites
    }

    fn stdlib(venv: &Path, sites: &[PathBuf]) -> Option<PathBuf> {
        let config = fs::read_to_string(venv.join(CONFIG)).ok()?;
        let home = PathBuf::from(Self::key(&config, HOME)?);
        let named = Self::key(&config, VERSION_INFO)
            .or_else(|| Self::key(&config, VERSION))
            .and_then(Self::short);
        let version = named.or_else(|| {
            sites.iter().find_map(|site| {
                let name = site.parent()?.file_name()?.to_string_lossy().into_owned();
                name.strip_prefix(PYTHON).map(str::to_owned)
            })
        })?;
        fs::canonicalize(home.parent()?.join(LIB).join(format!("{PYTHON}{version}"))).ok()
    }

    /// The environment of the project at a root: its `.venv`, the standard library its `pyvenv.cfg` names, and its
    /// site-packages; empty when it has no `.venv`.
    #[must_use]
    pub fn discover(root: &Path) -> Self {
        let venv = root.join(VENV);
        if !venv.is_dir() {
            return Self::default();
        }
        let sites = Self::sites(&venv);
        let stdlib = Self::stdlib(&venv, &sites);
        let roots = stdlib
            .iter()
            .flat_map(|stdlib| [stdlib.clone(), stdlib.join(DYNLOAD)])
            .filter(|root| root.is_dir())
            .chain(sites)
            .collect();
        Self {
            venv: Some(venv),
            stdlib,
            roots,
        }
    }
}

impl fmt::Display for Environment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(venv) = &self.venv else {
            return formatter.write_str("no .venv, no foreign roots");
        };
        let stdlib = self.stdlib.as_ref().map_or_else(
            || "no stdlib (pyvenv.cfg names none)".to_owned(),
            |path| path.display().to_string(),
        );
        write!(
            formatter,
            "{}: stdlib {stdlib}; {} foreign roots",
            venv.display(),
            self.roots.len()
        )
    }
}
