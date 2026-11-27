use std::env::{self, consts::EXE_SUFFIX};
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::fault::Failure;

const VENV: [&str; 2] = [".venv", "bin"];
const PATH: &str = "PATH";
const VERSION: &str = "--version";
const LINES: usize = 5;

/// An external tool e-law commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tool {
    /// The ruff linter.
    Ruff,
    /// The ty type checker.
    Ty,
    /// The tach module-boundary checker.
    Tach,
}

impl Tool {
    fn file(self) -> String {
        format!("{}{EXE_SUFFIX}", self.name())
    }

    /// The first non-blank lines of a text, joined by ` / `.
    pub(crate) fn head(text: &[u8]) -> String {
        String::from_utf8_lossy(text)
            .lines()
            .filter(|line| !line.trim().is_empty())
            .take(LINES)
            .collect::<Vec<_>>()
            .join(" / ")
    }

    /// Its executable's name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Ruff => "ruff",
            Self::Ty => "ty",
            Self::Tach => "tach",
        }
    }

    /// Where it is: beside the running `law`, its links resolved, then in the project's `.venv/bin/`, then on `PATH`.
    #[must_use]
    pub fn locate(self, root: &Path) -> Option<PathBuf> {
        let file = self.file();
        let beside = env::current_exe()
            .and_then(fs::canonicalize)
            .ok()
            .and_then(|law| law.parent().map(Path::to_owned));
        let venv = VENV.iter().fold(root.to_owned(), |dir, part| dir.join(part));
        let path = env::var_os(PATH)
            .map(|path| env::split_paths(&path).collect::<Vec<_>>())
            .unwrap_or_default();
        beside
            .into_iter()
            .chain([venv])
            .chain(path)
            .map(|dir| dir.join(&file))
            .find(|candidate| candidate.is_file())
    }

    /// What the tool writes on stdout run with `args` in `cwd`; any exit but success is a failure carrying stderr's
    /// first lines.
    ///
    /// # Errors
    /// When the tool does not start, or does not exit with success.
    pub fn run(self, path: &Path, cwd: &Path, args: &[&OsStr]) -> Result<Vec<u8>, Failure> {
        let output = self.output(path, cwd, args)?;
        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(Failure::Exit {
                code: output.status.code(),
                stderr: Self::head(&output.stderr),
            })
        }
    }

    /// What the tool exits with and writes, run with `args` in `cwd`, whatever its exit.
    ///
    /// # Errors
    /// When the tool does not start.
    pub fn output(self, path: &Path, cwd: &Path, args: &[&OsStr]) -> Result<Output, Failure> {
        Command::new(path)
            .args(args)
            .current_dir(cwd)
            .output()
            .map_err(|error| Failure::Spawn {
                path: path.to_owned(),
                message: error.to_string(),
            })
    }

    /// Its version, as `<tool> --version` reports it: the word after the name.
    ///
    /// # Errors
    /// When the tool does not run.
    pub fn version(self, path: &Path, cwd: &Path) -> Result<String, Failure> {
        let out = self.run(path, cwd, &[VERSION.as_ref()])?;
        let text = String::from_utf8_lossy(&out);
        let line = text.lines().next().unwrap_or_default().trim();
        Ok(line.split_whitespace().nth(1).unwrap_or(line).to_owned())
    }
}

impl fmt::Display for Tool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}
