mod common;

use common::{Outcome, Project};
use serde_json::{Value, json};

const RULES: &str = "- id: t.swap
  message: use new
  tags: {law: \"5\", set: LEXIS}
  scope: {include: [\"src/**/*.py\"], exclude: [\"src/gen/**\"]}
  check:
    engine: pattern
    rule: {pattern: \"old($A)\"}
    fix: \"new($A)\"
  fix: {safety: safe}
  cases:
    valid: [\"new(1)\\n\"]
    invalid: [{path: \"src/x.py\", code: \"old(1)\\nold(2)\\n\"}]
    fixed: [{code: \"old(1)\\n\", expect: \"new(1)\\n\"}]
- id: t.loop
  message: no loop
  scope: {include: [\"src/**/*.py\"]}
  severity: warning
  check: {engine: pattern, rule: {kind: for_statement}}
  fix: {safety: none, reason: loops are fine here}
- id: t.lambda
  message: no lambda
  check: {engine: pattern, rule: {kind: lambda}}
  fix: {safety: unsafe}
- id: u.other
  message: other
  check: {engine: pattern, rule: {kind: lambda}}
  fix: {safety: none, reason: x}
";

fn project() -> Result<Project, Box<dyn std::error::Error>> {
    Project::with_rules("[\"src\"]", RULES)
}

#[test]
fn rule_text_shows_every_part_of_the_rule() -> Outcome {
    let out = project()?.run(&["rule", "t.swap"])?;
    assert_eq!(
        out.stdout,
        "id: t.swap
message: use new
tags: law=5 set=LEXIS
severity: error
engine: pattern
fix: safe
scope: include src/**/*.py; exclude src/gen/**
valid:
  1. src/case.py
    new(1)
invalid:
  1. src/x.py
    old(1)
    old(2)
fixed:
  1. src/case.py
    code:
      old(1)
    expect:
      new(1)
"
    );
    assert_eq!((out.stderr.as_str(), out.code), ("", 0));
    Ok(())
}

#[test]
fn rule_text_names_the_reason_of_a_rule_with_no_fix_and_its_severity() -> Outcome {
    let out = project()?.run(&["rule", "t.loop"])?;
    assert!(out.stdout.contains("severity: warning\n"), "{}", out.stdout);
    assert!(
        out.stdout.contains("fix: none (loops are fine here)\n"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("valid: none\ninvalid: none\nfixed: none\n"),
        "{}",
        out.stdout
    );
    Ok(())
}

#[test]
fn rule_text_names_an_unsafe_fix_policy() -> Outcome {
    let out = project()?.run(&["rule", "t.lambda"])?;
    assert!(out.stdout.contains("fix: unsafe\n"), "{}", out.stdout);
    Ok(())
}

#[test]
fn rule_json_is_the_rule() -> Outcome {
    let out = project()?.run(&["rule", "t.swap", "--output-format", "json"])?;
    assert_eq!(out.code, 0);
    let rule: Value = serde_json::from_str(&out.stdout)?;
    assert_eq!(
        rule,
        json!({
            "id": "t.swap",
            "message": "use new",
            "tags": {"law": "5", "set": "LEXIS"},
            "scope": {"include": ["src/**/*.py"], "exclude": ["src/gen/**"]},
            "severity": "error",
            "check": {"engine": "pattern", "rule": {"pattern": "old($A)"}, "fix": "new($A)"},
            "fix": {"safety": "safe"},
            "cases": {
                "valid": [{"code": "new(1)\n"}],
                "invalid": [{"path": "src/x.py", "code": "old(1)\nold(2)\n"}],
                "fixed": [{"code": "old(1)\n", "expect": "new(1)\n"}]
            }
        })
    );
    Ok(())
}

#[test]
fn rule_listing_is_one_line_per_rule_in_catalogue_order() -> Outcome {
    let out = project()?.run(&["rule"])?;
    assert_eq!(
        out.stdout,
        "t.swap  [safe]  use new\nt.loop  [none]  no loop\nt.lambda  [unsafe]  no lambda\nu.other  [none]  other\n"
    );
    assert_eq!(out.code, 0);
    Ok(())
}

#[test]
fn rule_listing_follows_the_selection() -> Outcome {
    let project = project()?;
    assert_eq!(
        project.run(&["rule", "--select", "t.*", "--ignore", "t.loop"])?.stdout,
        "t.swap  [safe]  use new\nt.lambda  [unsafe]  no lambda\n"
    );
    assert_eq!(project.run(&["rule", "--select", "nope.*"])?.stdout, "");
    Ok(())
}

#[test]
fn rule_listing_json_is_the_selected_rules() -> Outcome {
    let out = project()?.run(&[
        "rule",
        "--select",
        "t.loop",
        "--select",
        "u.*",
        "--output-format",
        "json",
    ])?;
    let rules: Value = serde_json::from_str(&out.stdout)?;
    let ids: Vec<&str> = rules
        .as_array()
        .ok_or("not an array")?
        .iter()
        .filter_map(|rule| rule["id"].as_str())
        .collect();
    assert_eq!(ids, ["t.loop", "u.other"]);
    Ok(())
}

#[test]
fn rule_unknown_id_exits_2_listing_the_nearest_of_its_namespace() -> Outcome {
    let out = project()?.run(&["rule", "t.sawp"])?;
    assert_eq!(out.code, 2);
    assert_eq!(out.stdout, "");
    assert_eq!(
        out.stderr,
        "rule `t.sawp` not found; nearest: t.swap, t.loop, t.lambda\n"
    );
    Ok(())
}

#[test]
fn rule_unknown_id_in_an_unknown_namespace_says_so() -> Outcome {
    let out = project()?.run(&["rule", "x.y"])?;
    assert_eq!(out.code, 2);
    assert_eq!(out.stderr, "rule `x.y` not found; no rule in namespace `x`\n");
    Ok(())
}

#[test]
fn rule_malformed_id_is_a_usage_error() -> Outcome {
    let out = project()?.run(&["rule", "NotAnId"])?;
    assert_eq!(out.code, 2);
    assert_eq!(out.stdout, "");
    assert!(!out.stderr.is_empty());
    Ok(())
}

#[test]
fn rule_reserved_ids_are_not_catalogue_rules() -> Outcome {
    let out = project()?.run(&["rule", "law.pragma"])?;
    assert_eq!(out.code, 2);
    Ok(())
}

#[test]
fn rule_untagged_rule_prints_tags_none_and_a_tagged_one_its_tags() -> Outcome {
    let project = project()?;
    let untagged = project.run(&["rule", "t.lambda"])?;
    assert!(untagged.stdout.contains("\ntags: none\n"), "{}", untagged.stdout);
    let tagged = project.run(&["rule", "t.swap"])?;
    assert!(!tagged.stdout.contains("tags: none"), "{}", tagged.stdout);
    assert!(tagged.stdout.contains("\ntags: "), "{}", tagged.stdout);
    Ok(())
}

const EXTERNAL: &str = "- id: t.print
  message: no print
  check: {engine: ruff, code: T201}
  fix: {safety: none, reason: x}
- id: t.environ
  message: no environ
  check: {engine: ruff, code: TID251, message: \"os.environ\"}
  fix: {safety: none, reason: x}
- id: t.ret
  message: wrong return
  check: {engine: ty, rule: invalid-explicit-return}
  fix: {safety: none, reason: x}
- id: t.any
  message: type error
  check: {engine: ty}
  fix: {safety: none, reason: x}
- id: t.shape
  message: shape
  check: {engine: shape, classes: {count: 1}}
  fix: {safety: none, reason: x}
- id: t.members
  message: genealogy
  check: {engine: members, order: privates-first}
  fix: {safety: none, reason: x}
- id: t.tree
  message: tree
  check: {engine: tree, dirs: [\"src/*\"]}
  fix: {safety: none, reason: x}
- id: t.cycle
  message: cycle
  check: {engine: graph, aspect: cycle, typing: true}
  fix: {safety: none, reason: x}
- id: t.orphan
  message: orphan
  check: {engine: graph, aspect: orphan, entries: [\"src/*/main.py\", \"**/__main__.py\"]}
  fix: {safety: none, reason: x}
- id: t.bare
  message: bare
  check: {engine: graph, aspect: orphan}
  fix: {safety: none, reason: x}
";

#[test]
fn rule_text_names_the_engine_of_an_external_rule_with_its_code_rule_and_message() -> Outcome {
    let project = Project::with_rules("[\"src\"]", EXTERNAL)?;
    for (id, engine) in [
        ("t.print", "ruff T201"),
        ("t.environ", "ruff TID251 /os.environ/"),
        ("t.ret", "ty invalid-explicit-return"),
        ("t.any", "ty"),
        ("t.shape", "shape classes"),
        ("t.members", "members order"),
        ("t.tree", "tree src/*"),
        ("t.cycle", "graph cycle typing"),
        ("t.orphan", "graph orphan entries src/*/main.py, **/__main__.py"),
        ("t.bare", "graph orphan"),
    ] {
        let out = project.run(&["rule", id])?;
        assert_eq!((out.stderr.as_str(), out.code), ("", 0), "{id}");
        let lines: Vec<&str> = out.stdout.lines().collect();
        assert_eq!(
            lines.get(4).copied(),
            Some(format!("engine: {engine}").as_str()),
            "{id}: {}",
            out.stdout
        );
    }
    Ok(())
}

#[test]
fn rule_json_of_an_external_rule_keeps_its_spec() -> Outcome {
    let project = Project::with_rules("[\"src\"]", EXTERNAL)?;
    let out = project.run(&["rule", "t.environ", "--output-format", "json"])?;
    let rule: Value = serde_json::from_str(&out.stdout)?;
    assert_eq!(
        rule["check"],
        json!({"engine": "ruff", "code": "TID251", "message": "os.environ"})
    );
    Ok(())
}

const FORMER: &str = "- id: t.new
  message: renamed
  formerly: [old-one, old-two]
  check: {engine: pattern, rule: {kind: lambda}}
  fix: {safety: none, reason: x}
";

#[test]
fn rule_text_shows_the_former_ids_after_the_id() -> Outcome {
    let out = Project::with_rules("[\"src\"]", FORMER)?.run(&["rule", "t.new"])?;
    assert!(
        out.stdout
            .starts_with("id: t.new\nformerly: old-one, old-two\nmessage: renamed\n"),
        "{}",
        out.stdout
    );
    Ok(())
}

#[test]
fn rule_asked_by_a_former_id_answers_with_the_new_rule() -> Outcome {
    let project = Project::with_rules("[\"src\"]", FORMER)?;
    let named = project.run(&["rule", "t.new"])?;
    let asked = project.run(&["rule", "old-two"])?;
    assert_eq!(asked.stdout, named.stdout);
    assert_eq!((asked.stderr.as_str(), asked.code), ("", 0));
    let json = project.run(&["rule", "old-one", "--output-format", "json"])?;
    let rule: Value = serde_json::from_str(&json.stdout)?;
    assert_eq!(rule["id"], json!("t.new"));
    assert_eq!(rule["formerly"], json!(["old-one", "old-two"]));
    Ok(())
}

#[test]
fn rule_with_no_former_id_prints_no_formerly_line() -> Outcome {
    let out = project()?.run(&["rule", "t.swap"])?;
    assert!(!out.stdout.contains("formerly"), "{}", out.stdout);
    Ok(())
}
