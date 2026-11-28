#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

pub type Outcome = Result<(), Box<dyn std::error::Error>>;

pub const RULE: &str =
    "- id: lexis.elif\n  message: m\n  check: {engine: pattern, rule: {kind: x}}\n  fix: {safety: safe}\n";

pub fn write(root: &Path, relative: &str, text: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, text)?;
    Ok(path)
}

pub fn rule(id: &str) -> String {
    RULE.replace("lexis.elif", id)
}

pub fn manifest(root: &Path, dir: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    write(
        root,
        &format!("{dir}/catalogue.toml"),
        &format!("name = \"{dir}\"\nversion = \"1\"\ndescription = \"d\"\n"),
    )?;
    Ok(root.join(dir))
}
