mod common;

use common::{Outcome, Project, passed};
use serde_json::json;

const RULES: &str = "- id: lexis.elif
  message: no elif
  tags: {law: \"5\"}
  check: {engine: pattern, rule: {kind: elif_clause}}
  fix: {safety: none, reason: x}
- id: lexis.loop
  message: no loop
  tags: {law: \"6\"}
  check: {engine: pattern, rule: {kind: for_statement}}
  fix: {safety: none, reason: x}
- id: ost.lambda
  message: no lambda
  tags: {law: \"5\"}
  check: {engine: pattern, rule: {kind: lambda}}
  fix: {safety: none, reason: x}
";

const CODE: &str = "for i in x:\n    pass\nif a:\n    pass\nelif b:\n    pass\nf = lambda: 1\n";

const LOOP: &str = "src/a.py:1:1: lexis.loop no loop\n";
const ELIF: &str = "src/a.py:5:1: lexis.elif no elif\n";
const LAMBDA: &str = "src/a.py:7:5: ost.lambda no lambda\n";

fn project() -> Result<Project, Box<dyn std::error::Error>> {
    let project = Project::with_rules("[\"src\"]", RULES)?;
    project.file("src/a.py", CODE)?;
    Ok(project)
}

fn summary(count: usize) -> String {
    format!(
        "\nFound {} ({}, 0 warnings).\n",
        common::noun(count, "finding", "findings"),
        common::noun(count, "error", "errors")
    )
}

#[test]
fn selection_all_by_default_runs_every_rule() -> Outcome {
    let out = project()?.check(&[])?;
    assert_eq!(out.stdout, format!("{LOOP}{ELIF}{LAMBDA}{}", summary(3)));
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn selection_select_tag_membership() -> Outcome {
    let out = project()?.check(&["--select", "law=5"])?;
    assert_eq!(out.stdout, format!("{ELIF}{LAMBDA}{}", summary(2)));
    Ok(())
}

#[test]
fn selection_ignore_id_glob() -> Outcome {
    let out = project()?.check(&["--ignore", "lexis.*"])?;
    assert_eq!(out.stdout, format!("{LAMBDA}{}", summary(1)));
    Ok(())
}

#[test]
fn selection_ignore_beats_select() -> Outcome {
    let out = project()?.check(&["--select", "law=5", "--ignore", "lexis.elif"])?;
    assert_eq!(out.stdout, format!("{LAMBDA}{}", summary(1)));
    let all = project()?.check(&["--select", "ALL", "--ignore", "ALL"])?;
    assert!(passed(&all));
    Ok(())
}

#[test]
fn selection_select_exact_id_and_repeated_flags_union() -> Outcome {
    let one = project()?.check(&["--select", "lexis.loop"])?;
    assert_eq!(one.stdout, format!("{LOOP}{}", summary(1)));
    let two = project()?.check(&["--select", "lexis.loop", "--select", "ost.lambda"])?;
    assert_eq!(two.stdout, format!("{LOOP}{LAMBDA}{}", summary(2)));
    Ok(())
}

#[test]
fn selection_matching_nothing_passes() -> Outcome {
    assert!(passed(&project()?.check(&["--select", "nope.*"])?));
    Ok(())
}

#[test]
fn selection_config_select_and_ignore_apply() -> Outcome {
    let project = project()?;
    project.config("roots = [\"src\"]\ncatalogues = [\"cat/\"]\nselect = [\"law=5\"]\nignore = [\"ost.*\"]\n")?;
    let out = project.check(&[])?;
    assert_eq!(out.stdout, format!("{ELIF}{}", summary(1)));
    Ok(())
}

#[test]
fn selection_command_line_replaces_the_configuration() -> Outcome {
    let project = project()?;
    project.config("roots = [\"src\"]\ncatalogues = [\"cat/\"]\nselect = [\"law=5\"]\nignore = [\"ost.*\"]\n")?;
    let out = project.check(&["--select", "lexis.loop"])?;
    assert_eq!(out.stdout, format!("{LOOP}{}", summary(1)));
    let ignored = project.check(&["--ignore", "lexis.*"])?;
    assert_eq!(ignored.stdout, format!("{LAMBDA}{}", summary(1)));
    Ok(())
}

#[test]
fn selection_is_echoed_in_the_json_run() -> Outcome {
    let report = project()?.json(&["--select", "law=5", "--ignore", "lexis.elif"])?;
    assert_eq!(report["run"]["select"], json!(["law=5"]));
    assert_eq!(report["run"]["ignore"], json!(["lexis.elif"]));
    Ok(())
}

#[test]
fn selection_bad_selector_exits_2() -> Outcome {
    for bad in ["Lexis", "law=", "="] {
        let out = project()?.check(&["--select", bad])?;
        assert_eq!(out.code, 2, "selector {bad:?}: {out:?}");
        assert_eq!(out.stdout, "");
        assert!(!out.stderr.is_empty());
    }
    Ok(())
}
