use std::env;
use std::fmt::Display;
use std::fs;
use std::path::{Path, PathBuf};

use law_config::{Config, Loaded, Project, Selection, Source, discover, load};
use law_engine::{BatchEngine, CaseEngine};
use law_external::{RuffEngine, TachEngine, TyEngine};
use law_model::Rooted;

use crate::builtin::Builtin;
use crate::cli::ProjectArgs;
use crate::exit::Exit;

/// A project opened for a command: the working directory, what was loaded, and the rules selected.
pub struct Opened {
    /// The working directory.
    pub cwd: PathBuf,
    /// The configuration and its catalogues and exceptions.
    pub loaded: Loaded,
    /// The rules selected.
    pub selection: Selection,
    roots: Vec<PathBuf>,
}

impl Opened {
    fn roots(root: &Path) -> Vec<PathBuf> {
        let canonical = fs::canonicalize(root).ok();
        [Some(root.to_owned()), canonical].into_iter().flatten().collect()
    }

    fn rooted<F: Rooted>(roots: &[PathBuf], fault: F) -> F {
        roots.iter().fold(fault, |fault, root| fault.rooted(root))
    }

    /// Every fault on stderr, its paths relative to the project root where inside it; the exit of a fault.
    pub fn faults<F: Rooted + Display>(&self, faults: impl IntoIterator<Item = F>) -> Exit {
        Exit::faults(faults.into_iter().map(|fault| Self::rooted(&self.roots, fault)))
    }

    fn boxed<E: BatchEngine + 'static>(engine: Option<E>) -> Option<Box<dyn BatchEngine>> {
        engine.map(|engine| Box::new(engine) as Box<dyn BatchEngine>)
    }

    /// The external engines the selected rules need, ruff's, ty's then tach's; the exit of a fault when a tool is
    /// missing.
    ///
    /// # Errors
    /// The exit of a fault, its faults already on stderr.
    pub fn batch(&self) -> Result<Vec<Box<dyn BatchEngine>>, Exit> {
        let engines = [
            RuffEngine::new(&self.loaded, &self.selection).map(Self::boxed),
            TyEngine::new(&self.loaded, &self.selection).map(Self::boxed),
            TachEngine::new(&self.loaded, &self.selection).map(Self::boxed),
        ];
        let (found, faults): (Vec<_>, Vec<_>) = engines.into_iter().partition(Result::is_ok);
        if faults.is_empty() {
            Ok(found.into_iter().flatten().flatten().collect())
        } else {
            Err(self.faults(faults.into_iter().filter_map(Result::err)))
        }
    }

    /// The engines that prove external rules against their cases: ruff's; the exit of a fault when it is missing.
    ///
    /// # Errors
    /// The exit of a fault, its faults already on stderr.
    pub fn cases(&self) -> Result<Vec<Box<dyn CaseEngine>>, Exit> {
        RuffEngine::new(&self.loaded, &self.selection)
            .map(|linter| {
                linter
                    .map(|engine| Box::new(engine) as Box<dyn CaseEngine>)
                    .into_iter()
                    .collect()
            })
            .map_err(|fault| self.faults([fault]))
    }

    /// The project root.
    pub fn root(&self) -> &Path {
        &self.loaded.project.root
    }
}

impl ProjectArgs {
    fn project(&self, cwd: &Path, catalogues: &[Source]) -> Result<Project, Exit> {
        let found = self.config.as_ref().map_or_else(
            || discover(cwd),
            |path| Project::read(&cwd.join(path)).map_err(|fault| vec![fault]),
        );
        let given = (!catalogues.is_empty()).then(|| {
            catalogues
                .iter()
                .map(|source| match source {
                    Source::Dir(dir) => Source::Dir(cwd.join(dir)),
                    Source::Builtin(_) => source.clone(),
                })
                .collect::<Vec<_>>()
        });
        match (found, given) {
            (Ok(Some(mut project)), Some(given)) => {
                project.config.catalogues = given;
                Ok(project)
            }
            (Ok(Some(project)), None) => Ok(project),
            (Ok(None), Some(given)) => Ok(Project {
                root: cwd.to_owned(),
                path: cwd.to_owned(),
                config: Config {
                    catalogues: given,
                    ..Config::default()
                },
            }),
            (Ok(None), None) => Err(Exit::faults([format!(
                "{}: no law.toml, nor a pyproject.toml with [tool.law], here or above",
                cwd.display()
            )])),
            (Err(faults), _) => Err(Exit::faults(faults)),
        }
    }

    /// The project discovered or named, loaded, and its selection, the command line's replacing the configuration's;
    /// catalogues given replace the configuration's, and stand for it when there is none.
    ///
    /// # Errors
    /// The exit of a fault, its faults already on stderr.
    pub fn open(&self, catalogues: &[Source]) -> Result<Opened, Exit> {
        let cwd = env::current_dir().map_err(|error| Exit::faults([error]))?;
        let project = self.project(&cwd, catalogues)?;
        let roots = Opened::roots(&project.root);
        let loaded = load(&project, &Builtin::resolve)
            .map_err(|faults| Exit::faults(faults.into_iter().map(|fault| Opened::rooted(&roots, fault))))?;
        let given = |selectors: &Vec<_>| (!selectors.is_empty()).then(|| selectors.clone());
        let selection = Selection::new(&project.config, given(&self.select), given(&self.ignore));
        Ok(Opened {
            cwd,
            loaded,
            selection,
            roots,
        })
    }
}
