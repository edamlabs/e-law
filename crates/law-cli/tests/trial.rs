mod common;

use common::{Outcome, Project};
use serde_json::{Value, json};

const GOOD: &str = "- id: t.elif
  message: no elif
  scope: {include: [\"src/**/*.py\"]}
  check: {engine: pattern, rule: {kind: elif_clause}}
  fix: {safety: none, reason: x}
  cases:
    valid: [\"x = a if b else c\\n\"]
    invalid: [\"if a:\\n    pass\\nelif b:\\n    pass\\n\"]
";

const SWAP: &str = "- id: t.swap
  message: use new
  scope: {include: [\"src/**/*.py\"]}
  check:
    engine: pattern
    rule: {pattern: \"old($A)\"}
    fix: \"new($A)\"
  fix: {safety: safe}
  cases:
    valid: [\"new(1)\\n\"]
    invalid: [\"old(1)\\n\"]
    fixed: [{code: \"old(1)\\n\", expect: \"new(1)\\n\"}]
";

fn project(rules: &str) -> Result<Project, Box<dyn std::error::Error>> {
    Project::with_rules("[\"src\"]", rules)
}

fn test(rules: &str, args: &[&str]) -> Result<common::Out, Box<dyn std::error::Error>> {
    let mut all = vec!["test"];
    all.extend_from_slice(args);
    project(rules)?.run(&all)
}

#[test]
fn trial_passing_rule_prints_only_the_summary_and_exits_0() -> Outcome {
    let out = test(GOOD, &[])?;
    assert_eq!(out.stdout, "Tested 1 rule, 2 cases: 0 failed, 0 untested.\n");
    assert_eq!(out.stderr, "");
    assert_eq!(out.code, 0);
    Ok(())
}

#[test]
fn trial_fixed_case_passes_when_the_fix_gives_the_expected_text() -> Outcome {
    let out = test(SWAP, &[])?;
    assert_eq!(out.stdout, "Tested 1 rule, 3 cases: 0 failed, 0 untested.\n");
    assert_eq!(out.code, 0);
    Ok(())
}

#[test]
fn trial_valid_case_that_is_caught_fails() -> Outcome {
    let rules = GOOD.replace(
        "valid: [\"x = a if b else c\\n\"]",
        "valid: [\"if a:\\n    pass\\nelif b:\\n    pass\\n\"]",
    );
    let out = test(&rules, &[])?;
    assert_eq!(
        out.stdout,
        "t.elif: valid case 1: expected no finding, found 1 (the first at 3:1)\n\nTested 1 rule, 2 cases: 1 failed, 0 untested.\n"
    );
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn trial_invalid_case_that_is_not_caught_fails() -> Outcome {
    let rules = GOOD.replace(
        "invalid: [\"if a:\\n    pass\\nelif b:\\n    pass\\n\"]",
        "invalid: [\"x = 1\\n\"]",
    );
    let out = test(&rules, &[])?;
    assert_eq!(
        out.stdout,
        "t.elif: invalid case 1: expected a finding, found none\n\nTested 1 rule, 2 cases: 1 failed, 0 untested.\n"
    );
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn trial_rule_without_an_invalid_case_fails() -> Outcome {
    let rules = GOOD.replace("    invalid: [\"if a:\\n    pass\\nelif b:\\n    pass\\n\"]\n", "");
    let out = test(&rules, &[])?;
    assert_eq!(out.code, 1, "{out:?}");
    assert!(
        out.stdout
            .contains("t.elif: invalid: no case: an unproven rule is not a rule"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("1 failed"), "{}", out.stdout);
    Ok(())
}

#[test]
fn trial_rule_with_no_cases_at_all_fails() -> Outcome {
    let rules = GOOD.split("  cases:").next().ok_or("no split")?;
    let out = test(rules, &[])?;
    assert_eq!(out.code, 1, "{out:?}");
    assert!(
        out.stdout.contains("no case: an unproven rule is not a rule"),
        "{}",
        out.stdout
    );
    Ok(())
}

#[test]
fn trial_fixable_rule_without_a_fixed_case_fails() -> Outcome {
    let rules = SWAP.replace("    fixed: [{code: \"old(1)\\n\", expect: \"new(1)\\n\"}]\n", "");
    let out = test(&rules, &[])?;
    assert_eq!(out.code, 1, "{out:?}");
    assert!(
        out.stdout
            .contains("t.swap: fixed: no case: an unproven rule is not a rule"),
        "{}",
        out.stdout
    );
    Ok(())
}

#[test]
fn trial_unsafe_fixable_rule_without_a_fixed_case_fails_too() -> Outcome {
    let rules = SWAP
        .replace("safety: safe", "safety: unsafe")
        .replace("    fixed: [{code: \"old(1)\\n\", expect: \"new(1)\\n\"}]\n", "");
    let out = test(&rules, &[])?;
    assert_eq!(out.code, 1, "{out:?}");
    assert!(out.stdout.contains("t.swap: fixed: no case"), "{}", out.stdout);
    Ok(())
}

#[test]
fn trial_unfixable_rule_needs_no_fixed_case() -> Outcome {
    assert_eq!(test(GOOD, &[])?.code, 0);
    Ok(())
}

#[test]
fn trial_fixed_case_with_the_wrong_expectation_fails() -> Outcome {
    let rules = SWAP.replace("expect: \"new(1)\\n\"", "expect: \"newer(1)\\n\"");
    let out = test(&rules, &[])?;
    assert_eq!(
        out.stdout,
        "t.swap: fixed case 1: the fix gives \"new(1)\\n\", expected \"newer(1)\\n\"\n\nTested 1 rule, 3 cases: 1 failed, 0 untested.\n"
    );
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn trial_fixed_case_still_caught_after_the_fix_fails() -> Outcome {
    let rules = SWAP
        .replace("fix: \"new($A)\"", "fix: \"old($A)\"")
        .replace("expect: \"new(1)\\n\"", "expect: \"old(1)\\n\"");
    let out = test(&rules, &[])?;
    assert_eq!(
        out.stdout,
        "t.swap: fixed case 1: found 1 (the first at 1:1) after the fix\n\nTested 1 rule, 3 cases: 1 failed, 0 untested.\n"
    );
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn trial_each_failure_kind_reports_its_own_line_in_order() -> Outcome {
    let rules = SWAP
        .replace("valid: [\"new(1)\\n\"]", "valid: [\"new(1)\\n\", \"old(2)\\n\"]")
        .replace("invalid: [\"old(1)\\n\"]", "invalid: [\"old(1)\\n\", \"z = 1\\n\"]")
        .replace("expect: \"new(1)\\n\"", "expect: \"nope\\n\"");
    let out = test(&rules, &[])?;
    assert_eq!(
        out.stdout,
        "t.swap: valid case 2: expected no finding, found 1 (the first at 1:1)\nt.swap: invalid case 2: expected a finding, found none\nt.swap: fixed case 1: the fix gives \"new(1)\\n\", expected \"nope\\n\"\n\nTested 1 rule, 5 cases: 3 failed, 0 untested.\n"
    );
    Ok(())
}

#[test]
fn trial_case_path_places_the_snippet_inside_or_outside_the_scope() -> Outcome {
    let rules = GOOD.replace(
        "valid: [\"x = a if b else c\\n\"]",
        "valid: [{path: \"docs/x.py\", code: \"if a:\\n    pass\\nelif b:\\n    pass\\n\"}]",
    );
    let out = test(&rules, &[])?;
    assert_eq!(out.stdout, "Tested 1 rule, 2 cases: 0 failed, 0 untested.\n", "{out:?}");
    let inside = GOOD.replace(
        "invalid: [\"if a:\\n    pass\\nelif b:\\n    pass\\n\"]",
        "invalid: [{path: \"docs/x.py\", code: \"if a:\\n    pass\\nelif b:\\n    pass\\n\"}]",
    );
    let out = test(&inside, &[])?;
    assert_eq!(
        out.stdout,
        "t.elif: invalid case 1: expected a finding, found none\n\nTested 1 rule, 2 cases: 1 failed, 0 untested.\n"
    );
    Ok(())
}

#[test]
fn trial_tree_rules_are_untested_with_their_why_not_failed() -> Outcome {
    let tree = "- id: t.logic\n  message: logic holds\n  check: {engine: tree, dirs: [\"src/*/logic\"], allow: [\"README.md\"]}\n  fix: {safety: none, reason: x}\n";
    let out = test(&format!("{GOOD}{tree}"), &[])?;
    assert_eq!(
        out.stdout,
        "t.logic: untested: needs a directory\n\nTested 1 rule, 2 cases: 0 failed, 1 untested.\n"
    );
    assert_eq!(out.code, 0);
    Ok(())
}

#[test]
fn trial_lists_every_untested_rule_in_catalogue_order_before_the_summary_and_beside_failures() -> Outcome {
    let tree = "- id: t.logic\n  message: logic holds\n  check: {engine: tree, dirs: [\"src/*/logic\"], allow: [\"README.md\"]}\n  fix: {safety: none, reason: x}\n";
    let ty = "- id: t.ty\n  message: types hold\n  check: {engine: ty}\n  fix: {safety: none, reason: x}\n";
    let broken = GOOD.replace(
        "invalid: [\"if a:\\n    pass\\nelif b:\\n    pass\\n\"]",
        "invalid: [\"x = 1\\n\"]",
    );
    let out = test(&format!("{tree}{broken}{ty}"), &[])?;
    assert_eq!(
        out.stdout,
        "t.elif: invalid case 1: expected a finding, found none\nt.logic: untested: needs a directory\nt.ty: untested: needs an environment\n\nTested 1 rule, 2 cases: 1 failed, 2 untested.\n",
        "{out:?}"
    );
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn trial_json_untested_names_each_rule_and_its_need() -> Outcome {
    let tree = "- id: t.logic\n  message: logic holds\n  check: {engine: tree, dirs: [\"src/*/logic\"]}\n  fix: {safety: none, reason: x}\n";
    let ty = "- id: t.ty\n  message: types hold\n  check: {engine: ty}\n  fix: {safety: none, reason: x}\n";
    let out = test(&format!("{tree}{ty}"), &["--output-format", "json"])?;
    let report: Value = serde_json::from_str(&out.stdout)?;
    assert_eq!(
        report["untested"],
        json!([{"rule": "t.logic", "need": "directory"}, {"rule": "t.ty", "need": "environment"}])
    );
    Ok(())
}

#[test]
fn trial_a_rule_with_no_untested_reason_prints_no_untested_line() -> Outcome {
    let out = test(GOOD, &[])?;
    assert!(!out.stdout.contains("untested:"), "{}", out.stdout);
    Ok(())
}

#[test]
fn trial_select_and_ignore_narrow_the_rules() -> Outcome {
    let broken = GOOD.replace(
        "invalid: [\"if a:\\n    pass\\nelif b:\\n    pass\\n\"]",
        "invalid: [\"x = 1\\n\"]",
    );
    let rules = format!("{broken}{SWAP}");
    let all = test(&rules, &[])?;
    assert_eq!(all.code, 1);
    let only = test(&rules, &["--select", "t.swap"])?;
    assert_eq!(only.stdout, "Tested 1 rule, 3 cases: 0 failed, 0 untested.\n");
    assert_eq!(only.code, 0);
    let ignored = test(&rules, &["--ignore", "t.elif"])?;
    assert_eq!(ignored.stdout, only.stdout);
    let none = test(&rules, &["--select", "nope.*"])?;
    assert_eq!(none.stdout, "Tested 0 rules, 0 cases: 0 failed, 0 untested.\n");
    assert_eq!(none.code, 0);
    Ok(())
}

#[test]
fn trial_needs_no_source_file_and_writes_nothing() -> Outcome {
    let project = project(SWAP)?;
    project.file("src/a.py", "old(1)\n")?;
    let out = project.run(&["test"])?;
    assert_eq!(out.code, 0);
    assert_eq!(std::fs::read(project.path().join("src/a.py"))?, b"old(1)\n");
    Ok(())
}

#[test]
fn trial_json_gives_the_same_facts_as_a_structure() -> Outcome {
    let rules = GOOD.replace(
        "invalid: [\"if a:\\n    pass\\nelif b:\\n    pass\\n\"]",
        "invalid: [\"x = 1\\n\"]",
    );
    let out = test(&rules, &["--output-format", "json"])?;
    assert_eq!(out.code, 1);
    let report: Value = serde_json::from_str(&out.stdout)?;
    assert_eq!(report["rules"], 1);
    assert_eq!(report["cases"], 2);
    assert_eq!(report["failed"], 1);
    assert_eq!(report["untested"], json!([]));
    assert_eq!(
        report["trials"],
        json!([
            {"rule": "t.elif", "kind": "valid", "index": 1, "pass": true},
            {"rule": "t.elif", "kind": "invalid", "index": 1, "pass": false, "detail": "expected a finding, found none"}
        ])
    );
    Ok(())
}

#[test]
fn trial_a_rule_that_cannot_compile_exits_2_naming_the_rule() -> Outcome {
    let rules = GOOD.replace("{kind: elif_clause}", "{pattern: \"def (\"}");
    let out = test(&rules, &[])?;
    assert_eq!(out.code, 2, "{out:?}");
    assert_eq!(out.stdout, "");
    assert!(out.stderr.starts_with("rule `t.elif`: "), "{}", out.stderr);
    Ok(())
}

#[test]
fn trial_without_a_project_exits_2() -> Outcome {
    let empty = Project::new()?;
    let out = empty.run(&["test"])?;
    assert_eq!(out.code, 2);
    assert_eq!(out.stdout, "");
    assert!(!out.stderr.is_empty());
    Ok(())
}
