#![allow(dead_code)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use law_model::Rule;
use serde_json::{Value, json};

pub type Outcome = Result<(), Box<dyn std::error::Error>>;

pub fn fixture(name: &str) -> Result<String, Box<dyn std::error::Error>> {
    Ok(fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name),
    )?)
}

pub fn rule(id: &str, check: &Value, fix: &Value) -> Result<Rule, Box<dyn std::error::Error>> {
    Ok(serde_json::from_value(json!({
        "id": id, "message": "m", "check": check, "fix": fix,
    }))?)
}

pub fn ruff(id: &str, code: &str, message: Option<&str>) -> Result<Rule, Box<dyn std::error::Error>> {
    let check = message.map_or_else(
        || json!({"engine": "ruff", "code": code}),
        |message| json!({"engine": "ruff", "code": code, "message": message}),
    );
    rule(id, &check, &json!({"safety": "safe"}))
}

pub fn ty(id: &str, name: Option<&str>) -> Result<Rule, Box<dyn std::error::Error>> {
    let check = name.map_or_else(|| json!({"engine": "ty"}), |name| json!({"engine": "ty", "rule": name}));
    rule(id, &check, &json!({"safety": "none", "reason": "ty has no fixes"}))
}

pub fn tach(id: &str, code: Option<&str>) -> Result<Rule, Box<dyn std::error::Error>> {
    let check = code.map_or_else(
        || json!({"engine": "tach"}),
        |code| json!({"engine": "tach", "code": code}),
    );
    rule(id, &check, &json!({"safety": "none", "reason": "tach has no fixes"}))
}

pub fn ids<'a>(rules: impl IntoIterator<Item = &'a Rule>) -> Vec<&'a str> {
    rules.into_iter().map(|rule| rule.id.as_str()).collect()
}

pub const TACH_RULES: &str = "- id: t.undeclared
  message: undeclared
  check: {engine: tach, code: UndeclaredDependency}
  fix: {safety: none, reason: x}
- id: t.layer
  message: layer
  check: {engine: tach, code: LayerViolation}
  fix: {safety: none, reason: x}
- id: t.cycle
  message: cycle
  check: {engine: tach, code: CircularDependency}
  fix: {safety: none, reason: x}
- id: t.external
  message: external
  check: {engine: tach, code: UndeclaredExternalDependency}
  fix: {safety: none, reason: x}
- id: t.elif
  message: elif
  check: {engine: pattern, rule: {kind: elif_clause}}
  fix: {safety: none, reason: x}
";

/// What a stub `tach` writes: stdout and stderr of `check`, then of `check-external`, with their exits.
#[derive(Default)]
pub struct Said {
    pub check_out: String,
    pub check_err: String,
    pub check_code: i32,
    pub ext_out: String,
    pub ext_err: String,
    pub ext_code: i32,
}

/// A project root holding a `tach` stub in `.venv/bin`, a catalogue of `rules`, and the files given.
pub struct Root {
    pub dir: tempfile::TempDir,
    pub loaded: law_config::Loaded,
}

impl Root {
    pub fn new(rules: &str, said: &Said, files: &[(&str, &str)]) -> Result<Self, Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        let put = |relative: &str, text: &str| -> Result<(), Box<dyn std::error::Error>> {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().ok_or("no parent")?)?;
            fs::write(path, text)?;
            Ok(())
        };
        put(
            "cat/catalogue.toml",
            "name = \"t\"\nversion = \"1\"\ndescription = \"d\"\n",
        )?;
        put("cat/rules/r.yml", rules)?;
        put("law.toml", "catalogues = [\"cat/\"]\n")?;
        for (relative, text) in files {
            put(relative, text)?;
        }
        for (name, text) in [
            ("co", &said.check_out),
            ("ce", &said.check_err),
            ("xo", &said.ext_out),
            ("xe", &said.ext_err),
        ] {
            put(&format!("data/{name}"), text)?;
        }
        let data = root.join("data");
        let script = format!(
            "#!/bin/sh\necho \"$PWD|$*\" >> {log}\ncase \"$1\" in\n--version) echo 'tach 9.9.9';;\ncheck) cat {data}/co; cat {data}/ce >&2; exit {cc};;\ncheck-external) cat {data}/xo; cat {data}/xe >&2; exit {xc};;\nesac\n",
            log = root.join("log").display(),
            data = data.display(),
            cc = said.check_code,
            xc = said.ext_code,
        );
        put(".venv/bin/tach", &script)?;
        fs::set_permissions(root.join(".venv/bin/tach"), fs::Permissions::from_mode(0o755))?;
        let project = law_config::Project::read(&root.join("law.toml"))?.ok_or("no project")?;
        let lookup = |_: &str| None;
        let loaded = law_config::load(&project, &lookup).map_err(|faults| format!("{faults:?}"))?;
        Ok(Self { dir, loaded })
    }

    pub fn selection(&self, select: &[&str]) -> Result<law_config::Selection, Box<dyn std::error::Error>> {
        let chosen = select
            .iter()
            .map(|text| text.parse())
            .collect::<Result<Vec<law_model::Selector>, _>>()
            .map_err(|invalid| format!("{invalid:?}"))?;
        Ok(law_config::Selection::new(
            &self.loaded.project.config,
            Some(chosen),
            None,
        ))
    }

    pub fn log(&self) -> Vec<String> {
        fs::read_to_string(self.dir.path().join("log"))
            .map(|text| text.lines().map(str::to_owned).collect())
            .unwrap_or_default()
    }
}
