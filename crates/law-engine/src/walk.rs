use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use ignore::{DirEntry, WalkBuilder};

use crate::fault::Fault;
use crate::tree::SKIPPED;

pub(crate) const PYTHON: &str = "py";

/// What a walk found under the project root: Python files and directories, each from the root, in order.
#[derive(Debug, Default)]
pub struct Walk {
    /// The Python files.
    pub files: Vec<PathBuf>,
    /// The directories, the root itself left out.
    pub dirs: Vec<PathBuf>,
    /// The paths that could not be walked.
    pub faults: Vec<Fault>,
}

impl Walk {
    fn kept(entry: &DirEntry) -> bool {
        entry.file_name() != SKIPPED
    }

    fn take(&mut self, root: &Path, entry: Result<DirEntry, ignore::Error>) {
        match entry {
            Ok(entry) => match (entry.path().strip_prefix(root), entry.file_type()) {
                (Ok(relative), Some(kind)) if kind.is_dir() && !relative.as_os_str().is_empty() => {
                    self.dirs.push(relative.to_owned());
                }
                (Ok(relative), Some(kind)) if kind.is_file() && relative.extension() == Some(OsStr::new(PYTHON)) => {
                    self.files.push(relative.to_owned());
                }
                (Ok(_), _) => {}
                (Err(_), _) => self.faults.push(Fault::Outside {
                    path: entry.path().to_owned(),
                }),
            },
            Err(error) => self.faults.push(Fault::Io {
                path: root.to_owned(),
                message: error.to_string(),
            }),
        }
    }

    /// Every Python file and directory under the starts, `.gitignore` held, hidden entries and `__pycache__` skipped.
    #[must_use]
    pub fn new(root: &Path, starts: &[PathBuf]) -> Self {
        let mut walk = Self::default();
        let mut starts = starts.iter().map(|start| root.join(start));
        let Some(first) = starts.next() else {
            return walk;
        };
        let mut builder = WalkBuilder::new(first);
        starts.for_each(|start| {
            builder.add(start);
        });
        builder
            .require_git(false)
            .filter_entry(Self::kept)
            .build()
            .for_each(|entry| walk.take(root, entry));
        walk.files.sort();
        walk.files.dedup();
        walk.dirs.sort();
        walk.dirs.dedup();
        walk
    }
}
