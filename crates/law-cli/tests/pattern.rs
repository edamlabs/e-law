mod common;

use common::{ELIF, Outcome, Project, WITH_ELIF, found, passed};
use serde_json::json;

const SWAP: &str = "- id: t.swap
  message: use new
  scope: {include: [\"src/**/*.py\"]}
  check:
    engine: pattern
    rule: {pattern: \"old($A)\"}
    fix: \"new($A)\"
  fix: {safety: safe}
";

const LOOP: &str = "- id: t.loop
  message: no loop
  scope: {include: [\"src/**/*.py\"]}
  severity: warning
  check: {engine: pattern, rule: {kind: for_statement}}
  fix: {safety: none, reason: x}
";

#[test]
fn pattern_hit_reports_line_column_rule_message() -> Outcome {
    let project = Project::with_rules("[\"src\"]", ELIF)?;
    project.file("src/pkg/a.py", WITH_ELIF)?;
    let out = project.check(&[])?;
    assert_eq!(out.stdout, found("src/pkg/a.py:3:1", "t.elif", "no elif", 1, 0));
    assert_eq!(out.stderr, "");
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn pattern_miss_passes_clean() -> Outcome {
    let project = Project::with_rules("[\"src\"]", ELIF)?;
    project.file("src/pkg/a.py", "x = a if b else c\n")?;
    assert!(passed(&project.check(&[])?));
    Ok(())
}

#[test]
fn pattern_scope_include_and_exclude_fence_the_rule() -> Outcome {
    let rules = ELIF.replace(
        "scope: {include: [\"src/**/*.py\"]}",
        "scope: {include: [\"src/**/*.py\"], exclude: [\"**/ops/**\"]}",
    );
    let project = Project::with_rules("[\"src\", \"tests\"]", &rules)?;
    project.file("src/pkg/a.py", WITH_ELIF)?;
    project.file("src/pkg/ops/b.py", WITH_ELIF)?;
    project.file("tests/c.py", WITH_ELIF)?;
    let out = project.check(&[])?;
    assert_eq!(out.stdout, found("src/pkg/a.py:3:1", "t.elif", "no elif", 1, 0));
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn pattern_default_scope_is_every_python_file() -> Outcome {
    let rules = ELIF.replace("  scope: {include: [\"src/**/*.py\"]}\n", "");
    let project = Project::with_rules("[\"src\", \"tests\"]", &rules)?;
    project.file("src/a.py", WITH_ELIF)?;
    project.file("tests/c.py", WITH_ELIF)?;
    let out = project.check(&[])?;
    assert_eq!(
        out.stdout,
        "src/a.py:3:1: t.elif no elif\ntests/c.py:3:1: t.elif no elif\n\nFound 2 findings (2 errors, 0 warnings).\n"
    );
    Ok(())
}

#[test]
fn pattern_two_rules_one_file_one_run() -> Outcome {
    let project = Project::with_rules("[\"src\"]", &format!("{ELIF}{LOOP}"))?;
    project.file(
        "src/pkg/a.py",
        "for i in x:\n    pass\nif a:\n    pass\nelif b:\n    pass\n",
    )?;
    let out = project.check(&[])?;
    assert_eq!(
        out.stdout,
        "src/pkg/a.py:1:1: t.loop no loop\nsrc/pkg/a.py:5:1: t.elif no elif\n\nFound 2 findings (1 error, 1 warning).\n"
    );
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn pattern_fix_yields_the_edit_in_json() -> Outcome {
    let project = Project::with_rules("[\"src\"]", SWAP)?;
    project.file("src/a.py", "x = 1\nold(a.b)\n")?;
    let report = project.json(&[])?;
    let finding = &report["findings"][0];
    assert_eq!(finding["rule"], "t.swap");
    assert_eq!(finding["span"]["start"], json!({"line": 2, "column": 1}));
    assert_eq!(
        finding["fix"],
        json!({
            "safety": "safe",
            "edits": [{"edit": "replace", "path": "src/a.py", "bytes": {"start": 6, "end": 14}, "text": "new(a.b)"}]
        })
    );
    Ok(())
}

#[test]
fn pattern_fix_unsafe_policy_marks_the_fix_unsafe() -> Outcome {
    let project = Project::with_rules("[\"src\"]", &SWAP.replace("safety: safe", "safety: unsafe"))?;
    project.file("src/a.py", "old(1)\n")?;
    assert_eq!(project.json(&[])?["findings"][0]["fix"]["safety"], "unsafe");
    Ok(())
}

#[test]
fn pattern_fixless_rule_carries_no_fix_key() -> Outcome {
    let project = Project::with_rules("[\"src\"]", ELIF)?;
    project.file("src/a.py", WITH_ELIF)?;
    assert!(project.json(&[])?["findings"][0].get("fix").is_none());
    Ok(())
}

#[test]
fn pattern_bad_pattern_exits_2_naming_the_rule() -> Outcome {
    let rules = ELIF
        .replace("{kind: elif_clause}", "{pattern: \"def (\"}")
        .replace("t.elif", "t.bad");
    let project = Project::with_rules("[\"src\"]", &rules)?;
    project.file("src/a.py", WITH_ELIF)?;
    let out = project.check(&[])?;
    assert_eq!(out.code, 2);
    assert_eq!(out.stdout, "");
    assert_eq!(out.stderr.lines().count(), 1, "{}", out.stderr);
    assert!(out.stderr.starts_with("rule `t.bad`: "), "{}", out.stderr);
    Ok(())
}

#[test]
fn pattern_unknown_kind_exits_2_naming_the_rule() -> Outcome {
    let rules = ELIF
        .replace("elif_clause", "no_such_kind")
        .replace("t.elif", "t.nokind");
    let project = Project::with_rules("[\"src\"]", &rules)?;
    project.file("src/a.py", WITH_ELIF)?;
    let out = project.check(&[])?;
    assert_eq!(out.code, 2);
    assert_eq!(out.stdout, "");
    assert!(out.stderr.starts_with("rule `t.nokind`: "), "{}", out.stderr);
    Ok(())
}

#[test]
fn pattern_kindless_rule_exits_2_naming_the_rule() -> Outcome {
    let rules = ELIF
        .replace("{kind: elif_clause}", "{regex: \"foo\"}")
        .replace("t.elif", "t.kindless");
    let project = Project::with_rules("[\"src\"]", &rules)?;
    project.file("src/a.py", "foo = 1\n")?;
    let out = project.check(&[])?;
    assert_eq!(out.code, 2);
    assert_eq!(out.stdout, "");
    assert_eq!(out.stderr.lines().count(), 1, "{}", out.stderr);
    assert!(out.stderr.starts_with("rule `t.kindless`: "), "{}", out.stderr);
    Ok(())
}

#[test]
fn pattern_every_fault_is_collected_one_per_line() -> Outcome {
    let bad = ELIF.replace("{kind: elif_clause}", "{regex: \"foo\"}");
    let rules = format!("{}{}", bad.replace("t.elif", "t.one"), bad.replace("t.elif", "t.two"));
    let project = Project::with_rules("[\"src\"]", &rules)?;
    let out = project.check(&[])?;
    assert_eq!(out.code, 2);
    let lines: Vec<&str> = out.stderr.lines().collect();
    assert_eq!(lines.len(), 2, "{}", out.stderr);
    assert!(lines[0].starts_with("rule `t.one`: "), "{}", out.stderr);
    assert!(lines[1].starts_with("rule `t.two`: "), "{}", out.stderr);
    Ok(())
}

#[test]
fn pattern_fix_in_spec_under_policy_none_is_a_fault() -> Outcome {
    let rules = ELIF.replace("{kind: elif_clause}", "{kind: elif_clause}, fix: \"x\"");
    let project = Project::with_rules("[\"src\"]", &rules)?;
    let out = project.check(&[])?;
    assert_eq!(out.code, 2);
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr.starts_with("rule `t.elif`: a fix in the spec"),
        "{}",
        out.stderr
    );
    Ok(())
}

#[test]
fn pattern_safe_policy_without_fix_is_a_fault() -> Outcome {
    let rules = ELIF.replace("{safety: none, reason: x}", "{safety: safe}");
    let project = Project::with_rules("[\"src\"]", &rules)?;
    let out = project.check(&[])?;
    assert_eq!(out.code, 2);
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr.starts_with("rule `t.elif`: a fix in the spec"),
        "{}",
        out.stderr
    );
    Ok(())
}
