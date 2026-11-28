//! Embeds the built-in catalogue: every file of `catalogues/eager/` the loader reads, by its path from there.

use std::env;
use std::fs;
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};

const CATALOGUE: &str = "../../catalogues/eager";
const MANIFEST: &str = "catalogue.toml";
const DIRS: [&str; 2] = ["rules", "profiles"];
const YAML: &str = "yml";
const OUT: &str = "eager.rs";
const MANIFEST_DIR: &str = "CARGO_MANIFEST_DIR";
const OUT_DIR: &str = "OUT_DIR";

fn var(name: &str) -> io::Result<PathBuf> {
    env::var_os(name)
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(ErrorKind::NotFound, name))
}

fn text(path: &Path) -> io::Result<&str> {
    path.to_str()
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, path.display().to_string()))
}

fn yaml(dir: &Path, found: &mut Vec<PathBuf>) -> io::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    fs::read_dir(dir)?.try_for_each(|entry| {
        let path = entry?.path();
        if path.is_dir() {
            yaml(&path, found)
        } else {
            if path.extension().is_some_and(|extension| extension == YAML) {
                found.push(path);
            }
            Ok(())
        }
    })
}

fn main() -> io::Result<()> {
    let root = var(MANIFEST_DIR)?.join(CATALOGUE);
    let mut files = vec![root.join(MANIFEST)];
    DIRS.iter().try_for_each(|dir| yaml(&root.join(dir), &mut files))?;
    files.sort();
    let entries = files
        .iter()
        .map(|path| {
            let relative = path
                .strip_prefix(&root)
                .map_err(|error| io::Error::new(ErrorKind::InvalidInput, error))?;
            Ok(format!(
                "    ({:?}, include_str!({:?})),\n",
                text(relative)?,
                text(path)?
            ))
        })
        .collect::<io::Result<String>>()?;
    fs::write(var(OUT_DIR)?.join(OUT), format!("&[\n{entries}]\n"))?;
    writeln!(io::stdout(), "cargo::rerun-if-changed={}", root.display())
}
