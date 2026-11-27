#![allow(dead_code)]

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use tempfile::TempDir;

pub type Outcome = Result<(), Box<dyn std::error::Error>>;

pub const MANIFEST: &str = "name = \"t\"\nversion = \"1\"\ndescription = \"d\"\n";

pub const ELIF: &str = "- id: t.elif
  message: no elif
  tags: {law: \"5\"}
  scope: {include: [\"src/**/*.py\"]}
  check: {engine: pattern, rule: {kind: elif_clause}}
  fix: {safety: none, reason: x}
";

pub const WITH_ELIF: &str = "if a:\n    pass\nelif b:\n    pass\n";

#[derive(Debug)]
pub struct Out {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
}

pub struct Project {
    dir: TempDir,
}

impl Project {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            dir: tempfile::tempdir()?,
        })
    }

    pub fn with_rules(roots: &str, rules: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let project = Self::new()?;
        project.config(&format!("roots = {roots}\ncatalogues = [\"cat/\"]\n"))?;
        project.rules(rules)?;
        Ok(project)
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn file(&self, relative: &str, text: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        self.bytes(relative, text.as_bytes())
    }

    pub fn bytes(&self, relative: &str, bytes: &[u8]) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let path = self.dir.path().join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, bytes)?;
        Ok(path)
    }

    pub fn dir(&self, relative: &str) -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all(self.dir.path().join(relative))?;
        Ok(())
    }

    pub fn config(&self, text: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        self.file("law.toml", text)
    }

    pub fn rules(&self, yaml: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        self.file("cat/catalogue.toml", MANIFEST)?;
        self.file("cat/rules/r.yml", yaml)
    }

    pub fn run(&self, args: &[&str]) -> Result<Out, Box<dyn std::error::Error>> {
        let out = Command::new(env!("CARGO_BIN_EXE_law"))
            .current_dir(self.dir.path())
            .args(args)
            .output()?;
        Ok(Out {
            stdout: String::from_utf8(out.stdout)?,
            stderr: String::from_utf8(out.stderr)?,
            code: out.status.code().ok_or("killed by a signal")?,
        })
    }

    pub fn run_in(&self, cwd: &str, args: &[&str]) -> Result<Out, Box<dyn std::error::Error>> {
        let out = Command::new(env!("CARGO_BIN_EXE_law"))
            .current_dir(self.dir.path().join(cwd))
            .args(args)
            .output()?;
        Ok(Out {
            stdout: String::from_utf8(out.stdout)?,
            stderr: String::from_utf8(out.stderr)?,
            code: out.status.code().ok_or("killed by a signal")?,
        })
    }

    pub fn run_on(&self, path: &OsString, args: &[&str]) -> Result<Out, Box<dyn std::error::Error>> {
        let out = Command::new(env!("CARGO_BIN_EXE_law"))
            .current_dir(self.dir.path())
            .env("PATH", path)
            .args(args)
            .output()?;
        Ok(Out {
            stdout: String::from_utf8(out.stdout)?,
            stderr: String::from_utf8(out.stderr)?,
            code: out.status.code().ok_or("killed by a signal")?,
        })
    }

    pub fn run_on_in(&self, cwd: &str, path: &OsString, args: &[&str]) -> Result<Out, Box<dyn std::error::Error>> {
        let out = Command::new(env!("CARGO_BIN_EXE_law"))
            .current_dir(self.dir.path().join(cwd))
            .env("PATH", path)
            .args(args)
            .output()?;
        Ok(Out {
            stdout: String::from_utf8(out.stdout)?,
            stderr: String::from_utf8(out.stderr)?,
            code: out.status.code().ok_or("killed by a signal")?,
        })
    }

    pub fn check_on(&self, path: &OsString, args: &[&str]) -> Result<Out, Box<dyn std::error::Error>> {
        let mut all = vec!["check"];
        all.extend_from_slice(args);
        self.run_on(path, &all)
    }

    pub fn check(&self, args: &[&str]) -> Result<Out, Box<dyn std::error::Error>> {
        let mut all = vec!["check"];
        all.extend_from_slice(args);
        self.run(&all)
    }

    pub fn json(&self, args: &[&str]) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let mut all = vec!["--output-format", "json"];
        all.extend_from_slice(args);
        Ok(serde_json::from_str(&self.check(&all)?.stdout)?)
    }
}

pub fn passed(out: &Out) -> bool {
    out.stdout == "All checks passed.\n" && out.stderr.is_empty() && out.code == 0
}

pub fn noun(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

pub fn found(path_line: &str, rule: &str, message: &str, errors: usize, warnings: usize) -> String {
    let total = errors + warnings;
    format!(
        "{path_line}: {rule} {message}\n\nFound {} ({}, {}).\n",
        noun(total, "finding", "findings"),
        noun(errors, "error", "errors"),
        noun(warnings, "warning", "warnings"),
    )
}

pub const RUFF: &str = "ruff";
pub const TY: &str = "ty";
pub const TACH: &str = "tach";
const RUFF_PIN: &str = "0.17.6";
const TY_PIN: &str = "0.0.112";
const TACH_PIN: &str = "0.35.3";
const TOOLS_BIN: &str = "LAW_TOOLS_BIN";

static RUFF_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
static TY_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
static TACH_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

fn has(dir: &Path, tool: &str) -> bool {
    dir.join(tool).is_file()
}

fn on_path(tool: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path).find(|dir| has(dir, tool))
}

fn from_uv(tool: &str, pin: &str) -> Option<PathBuf> {
    let out = Command::new("uvx")
        .args(["--from", &format!("{tool}@{pin}"), "python", "-c"])
        .arg("import sysconfig; print(sysconfig.get_path('scripts'))")
        .output()
        .ok()?;
    let dir = PathBuf::from(String::from_utf8(out.stdout).ok()?.lines().last()?.trim());
    has(&dir, tool).then_some(dir)
}

fn dir_of(cell: &'static OnceLock<Option<PathBuf>>, tool: &str, pin: &str) -> Option<PathBuf> {
    cell.get_or_init(|| {
        env::var_os(TOOLS_BIN)
            .map(PathBuf::from)
            .filter(|dir| has(dir, tool))
            .or_else(|| on_path(tool))
            .or_else(|| from_uv(tool, pin))
    })
    .clone()
}

pub fn tool_dir(tool: &str) -> Option<PathBuf> {
    match tool {
        RUFF => dir_of(&RUFF_DIR, RUFF, RUFF_PIN),
        TACH => dir_of(&TACH_DIR, TACH, TACH_PIN),
        _ => dir_of(&TY_DIR, TY, TY_PIN),
    }
}

/// A PATH with the real tools first; none when one cannot be found.
pub fn path_with(tools: &[&str]) -> Option<OsString> {
    let dirs: Option<Vec<PathBuf>> = tools.iter().map(|tool| tool_dir(tool)).collect();
    let rest = env::var_os("PATH").map(|path| env::split_paths(&path).collect::<Vec<_>>());
    env::join_paths(dirs?.into_iter().chain(rest.unwrap_or_default())).ok()
}

pub fn skip(tool: &str) -> Outcome {
    writeln!(io::stderr(), "SKIP: {tool} cannot be found (PATH, {TOOLS_BIN}, uvx)")?;
    Ok(())
}

pub fn version_of(tool: &str) -> Result<String, Box<dyn std::error::Error>> {
    let dir = tool_dir(tool).ok_or("tool absent")?;
    let out = Command::new(dir.join(tool)).arg("--version").output()?;
    Ok(String::from_utf8(out.stdout)?
        .split_whitespace()
        .nth(1)
        .ok_or("no version")?
        .to_owned())
}

/// A directory holding an executable `name` that is the given shell script.
pub fn stub(dir: &Path, name: &str, script: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    fs::create_dir_all(dir)?;
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{script}\n"))?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// A PATH that holds only the directories given.
pub fn only(dirs: &[&Path]) -> Result<OsString, Box<dyn std::error::Error>> {
    Ok(env::join_paths(dirs)?)
}

pub const MIXED: &str = "- id: t.swap
  message: use new
  tags: {law: \"5\", set: [\"LEXIS\"], kit: \"py\"}
  scope: {include: [\"pk/**/*.py\"]}
  check:
    engine: pattern
    rule: {pattern: \"old($A)\"}
    fix: \"new($A)\"
  fix: {safety: safe}
- id: t.elif
  message: no elif
  tags: {law: \"10\", set: [\"LEXIS\", \"OST\"]}
  scope: {include: [\"pk/**/*.py\"]}
  check: {engine: pattern, rule: {kind: elif_clause}}
  fix: {safety: none, reason: x}
- id: t.lambda
  message: no lambda
  severity: warning
  tags: {law: \"2\"}
  scope: {include: [\"pk/**/*.py\"]}
  check: {engine: pattern, rule: {kind: lambda}}
  fix: {safety: none, reason: x}
- id: t.untagged
  message: untagged rule
  scope: {include: [\"pk/**/*.py\"]}
  check: {engine: pattern, rule: {pattern: \"print($A)\"}}
  fix: {safety: none, reason: x}
";

pub const MIXED_FILE: &str = "pk/dir \u{f1}/\u{f1}and\u{fa}.py";
pub const MIXED_CODE: &str = "x = 1\ndef f():\n    old(\u{e1}.b)\nif a:\n    pass\nelif b:\n    pass\nprint(1)\n";

/// Findings of four rules in two files, one path with a space and a non-ASCII character, one finding with a fix.
pub fn mixed() -> Result<Project, Box<dyn std::error::Error>> {
    let project = Project::with_rules("[\"pk\"]", MIXED)?;
    project.file(MIXED_FILE, MIXED_CODE)?;
    let lambdas = (0..25)
        .map(|index| format!("f{index} = lambda: {index}\n"))
        .collect::<Vec<_>>()
        .concat();
    project.file("pk/other/many.py", &lambdas)?;
    Ok(project)
}

/// A git command run in a directory with an identity of its own; its trimmed stdout.
pub fn git(dir: &Path, args: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
    let out = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .output()?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned().into());
    }
    Ok(String::from_utf8(out.stdout)?.trim().to_owned())
}

/// A command's output, run again while the kernel reports its executable still open for writing: a file just
/// copied or written is held by the child of a concurrent fork until that child execs.
pub fn output(command: &mut Command) -> io::Result<std::process::Output> {
    (0..50)
        .find_map(|_| match command.output() {
            Err(error) if error.kind() == io::ErrorKind::ExecutableFileBusy => {
                std::thread::sleep(std::time::Duration::from_millis(20));
                None
            }
            done => Some(done),
        })
        .unwrap_or_else(|| command.output())
}
