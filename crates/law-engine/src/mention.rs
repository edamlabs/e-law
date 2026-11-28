use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use ignore::WalkBuilder;

use crate::tree::SKIPPED;
use crate::walk::PYTHON;

const GIT: &str = "git";
const TRACKED: [&str; 2] = ["ls-files", "-z"];
const SEPARATOR: char = '\0';
const SIZE: u64 = 2 << 20;
const DOT: char = '.';
const DASH: char = '-';
const ESCAPED: &str = "\\.";

/// The text files of a project that are not Python: what git tracks, or, outside a repository, what the walk sees.
/// A file over two mebibytes, or that is not text, is not read: a lockfile or a model does not name a module. A dot
/// escaped for a regular expression is read as a dot: a filter of warnings names a class that way.
#[derive(Debug, Default)]
pub(crate) struct Others {
    files: Vec<(PathBuf, String)>,
}

impl Others {
    fn tracked(root: &Path) -> Option<Vec<PathBuf>> {
        let out = Command::new(GIT)
            .args(TRACKED)
            .current_dir(root)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        out.status.success().then(|| {
            String::from_utf8_lossy(&out.stdout)
                .split(SEPARATOR)
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
                .collect()
        })
    }

    fn walked(root: &Path) -> Vec<PathBuf> {
        WalkBuilder::new(root)
            .require_git(false)
            .filter_entry(|entry| entry.file_name() != SKIPPED)
            .build()
            .flatten()
            .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
            .filter_map(|entry| entry.path().strip_prefix(root).ok().map(Path::to_owned))
            .collect()
    }

    /// The others of the project at a root, by path.
    pub(crate) fn new(root: &Path) -> Self {
        let mut paths = Self::tracked(root).unwrap_or_else(|| Self::walked(root));
        paths.retain(|path| path.extension() != Some(OsStr::new(PYTHON)));
        paths.sort();
        let files = paths
            .into_iter()
            .filter(|path| fs::metadata(root.join(path)).is_ok_and(|meta| meta.is_file() && meta.len() <= SIZE))
            .filter_map(|path| {
                let text = fs::read_to_string(root.join(&path)).ok()?.replace(ESCAPED, ".");
                Some((path, text))
            })
            .collect();
        Self { files }
    }

    fn word(letter: char) -> bool {
        letter.is_alphanumeric() || letter == '_'
    }

    fn named(line: &str, name: &str, before: impl Fn(char) -> bool) -> bool {
        line.match_indices(name).any(|(at, _)| {
            let ahead = line[at + name.len()..].chars().next();
            line[..at].chars().next_back().is_none_or(|letter| !before(letter))
                && ahead.is_none_or(|letter| !Self::word(letter))
        })
    }

    /// The first line of each file that writes one of the dotted `names` of the module, or its old `path`, as a
    /// whole: `path:line`, in path order.
    pub(crate) fn mentions(&self, names: &[String], path: &Path) -> Vec<String> {
        let written = path.to_string_lossy();
        self.files
            .iter()
            .filter_map(|(file, text)| {
                let line = text.lines().position(|line| {
                    names
                        .iter()
                        .any(|name| Self::named(line, name, |letter| Self::word(letter) || letter == DOT))
                        || Self::named(line, &written, |letter| Self::word(letter) || letter == DASH)
                })?;
                Some(format!("{}:{}", file.display(), line + 1))
            })
            .collect()
    }
}
