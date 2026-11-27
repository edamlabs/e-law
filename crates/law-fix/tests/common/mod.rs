#![allow(dead_code)]

use std::fs;
use std::path::Path;

use law_model::Finding;
use serde_json::{Value, json};
use tempfile::TempDir;

pub type Outcome = Result<(), Box<dyn std::error::Error>>;

pub fn replace(path: &str, start: usize, end: usize, text: &str) -> Value {
    json!({"edit": "replace", "path": path, "bytes": {"start": start, "end": end}, "text": text})
}

pub fn finding(rule: &str, path: &str, safety: &str, edits: &[Value]) -> Result<Finding, Box<dyn std::error::Error>> {
    let start = edits
        .first()
        .and_then(|edit| edit["bytes"]["start"].as_u64())
        .unwrap_or_default();
    Ok(serde_json::from_value(json!({
        "rule": rule,
        "path": path,
        "span": {
            "start": {"line": 1, "column": 1},
            "end": {"line": 1, "column": 2},
            "bytes": {"start": start, "end": start + 1},
        },
        "message": "m",
        "severity": "error",
        "tags": {},
        "fix": {"safety": safety, "edits": edits},
    }))?)
}

pub fn safe(
    rule: &str,
    path: &str,
    start: usize,
    end: usize,
    text: &str,
) -> Result<Finding, Box<dyn std::error::Error>> {
    finding(rule, path, "safe", &[replace(path, start, end, text)])
}

pub fn tree(files: &[(&str, &[u8])]) -> Result<TempDir, Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    files.iter().try_for_each(|(name, bytes)| {
        let path = dir.path().join(name);
        path.parent().map(fs::create_dir_all).transpose()?;
        fs::write(path, bytes)
    })?;
    Ok(dir)
}

pub fn read(dir: &Path, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    Ok(fs::read_to_string(dir.join(name))?)
}
