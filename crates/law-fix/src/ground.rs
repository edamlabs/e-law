use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::fault::Fault;

/// Where a fix is settled before anything is written: what a path holds, whether one is there, whether it may be
/// written.
pub(crate) trait Ground {
    /// The path as a fault names it.
    fn locate(&self, path: &Path) -> PathBuf;

    /// What the path holds.
    fn read(&self, path: &Path) -> Result<String, Fault>;

    /// Whether anything is at the path.
    fn exists(&self, path: &Path) -> bool;

    /// Whether its owner left the file writable.
    fn writable(&self, path: &Path) -> bool;
}

/// The project tree on disk under a root.
pub(crate) struct Disk<'a>(pub(crate) &'a Path);

/// A tree held in memory, by path from its root.
pub(crate) struct Memory<'a>(pub(crate) &'a BTreeMap<PathBuf, String>);

impl Ground for Disk<'_> {
    fn locate(&self, path: &Path) -> PathBuf {
        self.0.join(path)
    }

    fn read(&self, path: &Path) -> Result<String, Fault> {
        let full = self.0.join(path);
        fs::read_to_string(&full).map_err(|error| Fault::io(&full, &error))
    }

    fn exists(&self, path: &Path) -> bool {
        self.0.join(path).symlink_metadata().is_ok()
    }

    fn writable(&self, path: &Path) -> bool {
        fs::metadata(self.0.join(path)).is_ok_and(|meta| !meta.permissions().readonly())
    }
}

impl Ground for Memory<'_> {
    fn locate(&self, path: &Path) -> PathBuf {
        path.to_owned()
    }

    fn read(&self, path: &Path) -> Result<String, Fault> {
        self.0.get(path).cloned().ok_or_else(|| Fault::Io {
            path: path.to_owned(),
            message: "no such file".to_owned(),
        })
    }

    fn exists(&self, path: &Path) -> bool {
        self.0.contains_key(path)
    }

    fn writable(&self, _: &Path) -> bool {
        true
    }
}
