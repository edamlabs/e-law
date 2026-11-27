use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use crate::fault::Fault;
use crate::tree::SKIPPED;

const META: [char; 4] = ['*', '?', '[', '{'];

/// Where a `parity` rule reads the files beside a declaring file or matching a glob: the project's disk, or the files
/// of one case and nothing else.
pub(crate) enum Listing<'a> {
    Disk(&'a Path),
    Case(&'a [PathBuf]),
}

impl Listing<'_> {
    /// The directory a glob can match under: the leading segments of it that name no pattern.
    pub(crate) fn prefix(glob: &str) -> PathBuf {
        glob.rsplit_once('/')
            .map_or("", |(dirs, _)| dirs)
            .split('/')
            .take_while(|segment| !segment.contains(META))
            .collect()
    }

    fn walked(root: &Path, under: &Path, faults: &mut Vec<Fault>) -> Vec<PathBuf> {
        let start = root.join(under);
        if !start.is_dir() {
            return Vec::new();
        }
        let mut files: Vec<PathBuf> = WalkBuilder::new(&start)
            .require_git(false)
            .filter_entry(|entry| entry.file_name() != SKIPPED)
            .build()
            .filter_map(|entry| {
                entry
                    .map_err(|error| {
                        faults.push(Fault::Io {
                            path: under.to_owned(),
                            message: error.to_string(),
                        });
                    })
                    .ok()
            })
            .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
            .filter_map(|entry| entry.path().strip_prefix(root).map(Path::to_owned).ok())
            .collect();
        files.sort();
        files
    }

    /// Every file at any depth under a directory, from the project root, sorted.
    pub(crate) fn files(&self, under: &Path, faults: &mut Vec<Fault>) -> Vec<PathBuf> {
        match self {
            Self::Disk(root) => Self::walked(root, under, faults),
            Self::Case(files) => files.iter().filter(|file| file.starts_with(under)).cloned().collect(),
        }
    }
}
