mod common;

use common::{Outcome, Project, passed};

const DANGER: &str = "- id: t.danger
  message: no danger
  scope: {include: [\"src/**/*.py\"]}
  check: {engine: pattern, rule: {pattern: \"danger($A)\"}}
  fix: {safety: none, reason: x}
";

const RENAMED: &str = "- id: t.danger
  message: no danger
  formerly: [danger-old]
  scope: {include: [\"src/**/*.py\"]}
  check: {engine: pattern, rule: {pattern: \"danger($A)\"}}
  fix: {safety: none, reason: x}
";

const PRAGMA: &str = "law.pragma silenced inline: register an exception instead";
const EXCEPTIONS: &str = "ex.toml";
const NAMED: &str = "danger(1)  # ast-grep-ignore: t.danger\n";

fn verdict(code: &str) -> Result<common::Out, Box<dyn std::error::Error>> {
    let project = Project::with_rules("[\"src\"]", DANGER)?;
    project.file("src/a.py", code)?;
    project.check(&[])
}

fn only_pragma(position: &str, detail: &str) -> String {
    format!("src/a.py:{position}: {PRAGMA}: {detail}\n\nFound 1 finding (1 error, 0 warnings).\n")
}

fn licence(rule: &str, path: &str, reach: &str, expires: Option<&str>) -> String {
    let expiry = expires.map_or_else(String::new, |day| format!("expires = \"{day}\"\n"));
    format!(
        "[[exception]]\nrule = \"{rule}\"\npath = \"{path}\"\nreach = \"{reach}\"\nreason = \"r\"\napproved = \"d\"\n{expiry}\n"
    )
}

fn licensed(exceptions: &str, code: &str) -> Result<common::Out, Box<dyn std::error::Error>> {
    let project = Project::with_rules("[\"src\"]", DANGER)?;
    project.config(&format!(
        "roots = [\"src\"]\ncatalogues = [\"cat/\"]\nexceptions = \"{EXCEPTIONS}\"\n"
    ))?;
    project.file(EXCEPTIONS, exceptions)?;
    project.file("src/a.py", code)?;
    project.check(&[])
}

#[test]
fn pragma_bare_trailing_comment_is_a_finding_and_the_run_fails() -> Outcome {
    let out = verdict("danger(1)  # ast-grep-ignore\n")?;
    assert_eq!(out.stdout, only_pragma("1:12", "names no rule"));
    assert_eq!(out.stderr, "");
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn pragma_bare_previous_line_comment_is_a_finding() -> Outcome {
    let out = verdict("x = 1\n# ast-grep-ignore\ndanger(1)\n")?;
    assert_eq!(out.stdout, only_pragma("2:1", "names no rule"));
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn pragma_bare_file_level_comment_is_one_finding() -> Outcome {
    let out = verdict("# ast-grep-ignore\n\ndanger(1)\ndanger(2)\n")?;
    assert_eq!(out.stdout, only_pragma("1:1", "names no rule"));
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn pragma_bare_on_a_line_with_no_match_is_still_a_finding() -> Outcome {
    let out = verdict("x = 1  # ast-grep-ignore\n")?;
    assert_eq!(out.stdout, only_pragma("1:8", "names no rule"));
    Ok(())
}

#[test]
fn pragma_each_comment_is_its_own_finding() -> Outcome {
    let out = verdict("danger(1)  # ast-grep-ignore\ny = 2\ndanger(3)  # ast-grep-ignore\n")?;
    assert_eq!(
        out.stdout,
        format!(
            "src/a.py:1:12: {PRAGMA}: names no rule\nsrc/a.py:3:12: {PRAGMA}: names no rule\n\nFound 2 findings (2 errors, 0 warnings).\n"
        )
    );
    Ok(())
}

#[test]
fn pragma_unknown_id_is_not_a_rule() -> Outcome {
    let out = verdict("danger(1)  # ast-grep-ignore: other.rule\n")?;
    assert!(
        out.stdout
            .contains(&format!("src/a.py:1:12: {PRAGMA}: other.rule is not a rule\n")),
        "{}",
        out.stdout
    );
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn pragma_malformed_id_is_not_a_rule() -> Outcome {
    let out = verdict("x = 1  # ast-grep-ignore: NotAnId\n")?;
    assert_eq!(out.stdout, only_pragma("1:8", "NotAnId is not a rule"));
    Ok(())
}

#[test]
fn pragma_known_id_without_exception_has_no_registered_exception() -> Outcome {
    let out = verdict(NAMED)?;
    assert!(
        out.stdout.contains(&format!(
            "src/a.py:1:12: {PRAGMA}: t.danger has no registered exception\n"
        )),
        "{}",
        out.stdout
    );
    assert!(!out.stdout.contains("t.danger no danger"), "{}", out.stdout);
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn pragma_each_id_of_a_list_is_judged_alone() -> Outcome {
    let out = verdict("x = 1  # ast-grep-ignore: t.danger, other.rule\n")?;
    assert_eq!(
        out.stdout,
        format!(
            "src/a.py:1:8: {PRAGMA}: other.rule is not a rule\nsrc/a.py:1:8: {PRAGMA}: t.danger has no registered exception\n\nFound 2 findings (2 errors, 0 warnings).\n"
        )
    );
    Ok(())
}

#[test]
fn pragma_licensed_by_a_pragma_exception_is_silent_and_silences() -> Outcome {
    let out = licensed(&licence("t.danger", "src/a.py", "pragma", None), NAMED)?;
    assert!(passed(&out), "{out:?}");
    Ok(())
}

#[test]
fn pragma_licence_selector_forms_license() -> Outcome {
    for selector in ["t.*", "ALL"] {
        let out = licensed(&licence(selector, "src/*.py", "pragma", None), NAMED)?;
        assert!(passed(&out), "selector {selector}: {out:?}");
    }
    Ok(())
}

#[test]
fn pragma_licence_of_another_path_does_not_license() -> Outcome {
    let out = licensed(&licence("t.danger", "src/b.py", "pragma", None), NAMED)?;
    assert!(
        out.stdout.contains("t.danger has no registered exception"),
        "{}",
        out.stdout
    );
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn pragma_licence_of_another_rule_does_not_license() -> Outcome {
    let out = licensed(&licence("t.other", "src/a.py", "pragma", None), NAMED)?;
    assert!(
        out.stdout.contains("t.danger has no registered exception"),
        "{}",
        out.stdout
    );
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn pragma_file_reach_exception_does_not_license() -> Outcome {
    let out = licensed(&licence("t.danger", "src/a.py", "file", None), NAMED)?;
    assert!(
        out.stdout.contains("t.danger has no registered exception"),
        "{}",
        out.stdout
    );
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn pragma_expired_licence_does_not_license_and_is_reported() -> Outcome {
    let out = licensed(&licence("t.danger", "src/a.py", "pragma", Some("2000-01-01")), NAMED)?;
    assert!(out.stdout.contains("law.exception-expired"), "{}", out.stdout);
    assert!(
        out.stdout.contains("t.danger has no registered exception"),
        "{}",
        out.stdout
    );
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn pragma_unused_licence_is_reported_in_a_whole_tree_run() -> Outcome {
    let out = licensed(&licence("t.danger", "src/a.py", "pragma", None), "x = 1\n")?;
    assert_eq!(
        out.stdout,
        format!(
            "{EXCEPTIONS}:1:1: law.exception-unused the exception silences nothing: remove it: `t.danger` at src/a.py\n\nFound 1 finding (1 error, 0 warnings).\n"
        )
    );
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn pragma_licence_does_not_silence_a_finding_by_itself() -> Outcome {
    let out = licensed(&licence("t.danger", "src/a.py", "pragma", None), "danger(1)\n")?;
    assert!(
        out.stdout.contains("src/a.py:1:1: t.danger no danger\n"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("law.exception-unused"), "{}", out.stdout);
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn pragma_prose_and_strings_are_not_pragmas() -> Outcome {
    let out = verdict("# see the ast-grep-ignore docs\nx = '# ast-grep-ignore'\ny = 1  # noqa ast-grep-ignore\n")?;
    assert!(passed(&out), "{out:?}");
    Ok(())
}

#[test]
fn pragma_is_reported_in_a_file_no_rule_looks_at() -> Outcome {
    let project = Project::with_rules("[\"src\", \"tests\"]", DANGER)?;
    project.dir("src")?;
    project.file("tests/t.py", "x = 1  # ast-grep-ignore\n")?;
    let out = project.check(&[])?;
    assert_eq!(
        out.stdout,
        format!("tests/t.py:1:8: {PRAGMA}: names no rule\n\nFound 1 finding (1 error, 0 warnings).\n")
    );
    Ok(())
}

#[test]
fn pragma_rule_cannot_be_ignored_or_deselected() -> Outcome {
    let project = Project::with_rules("[\"src\"]", DANGER)?;
    project.file("src/a.py", "x = 1  # ast-grep-ignore\n")?;
    for args in [
        vec!["--ignore", "law.pragma"],
        vec!["--ignore", "law.*"],
        vec!["--ignore", "ALL"],
        vec!["--select", "t.danger"],
        vec!["--select", "law.pragma"],
    ] {
        let out = project.check(&args)?;
        assert_eq!(out.stdout, only_pragma("1:8", "names no rule"), "{args:?}");
        assert_eq!(out.code, 1, "{args:?}");
    }
    Ok(())
}

#[test]
fn pragma_finding_is_not_silenced_by_an_exception() -> Outcome {
    let out = licensed(
        &licence("law.pragma", "src/a.py", "file", None),
        "x = 1  # ast-grep-ignore\n",
    )?;
    assert!(
        out.stdout.contains(&format!("src/a.py:1:8: {PRAGMA}: names no rule\n")),
        "{}",
        out.stdout
    );
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn pragma_finding_is_an_error_in_json_with_no_tags() -> Outcome {
    let project = Project::with_rules("[\"src\"]", DANGER)?;
    project.file("src/a.py", "x = 1  # ast-grep-ignore\n")?;
    let report = project.json(&[])?;
    let finding = &report["findings"][0];
    assert_eq!(finding["rule"], "law.pragma");
    assert_eq!(finding["severity"], "error");
    assert_eq!(finding["tags"], serde_json::json!({}));
    assert_eq!(
        finding["message"],
        "silenced inline: register an exception instead: names no rule"
    );
    Ok(())
}

fn renamed(code: &str) -> Result<Project, Box<dyn std::error::Error>> {
    let project = Project::with_rules("[\"src\"]", RENAMED)?;
    project.file("src/a.py", code)?;
    Ok(project)
}

#[test]
fn pragma_former_id_is_one_finding_naming_the_new_id_and_the_rule_still_fires() -> Outcome {
    let out = renamed("danger(1)  # ast-grep-ignore: danger-old\n")?.check(&[])?;
    assert_eq!(
        out.stdout,
        format!(
            "src/a.py:1:1: t.danger no danger\nsrc/a.py:1:12: {PRAGMA}: danger-old is now t.danger\n\nFound 2 findings (2 errors, 0 warnings).\n"
        )
    );
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn pragma_former_id_fix_respells_exactly_the_id_and_the_pragma_is_then_judged() -> Outcome {
    let project =
        renamed("danger(1)  # ast-grep-ignore: danger-old, other.rule\ny = 2  # ast-grep-ignore: danger-older\n")?;
    let out = project.check(&["--fix"])?;
    assert_eq!(
        std::fs::read_to_string(project.path().join("src/a.py"))?,
        "danger(1)  # ast-grep-ignore: t.danger, other.rule\ny = 2  # ast-grep-ignore: danger-older\n"
    );
    assert!(out.stdout.contains("Fixed 1 finding.\n"), "{}", out.stdout);
    assert!(
        out.stdout
            .contains(&format!("src/a.py:1:12: {PRAGMA}: other.rule is not a rule\n")),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout.contains(&format!(
            "src/a.py:1:12: {PRAGMA}: t.danger has no registered exception\n"
        )),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout
            .contains(&format!("src/a.py:2:8: {PRAGMA}: danger-older is not a rule\n")),
        "{}",
        out.stdout
    );
    assert!(!out.stdout.contains("is now"), "{}", out.stdout);
    Ok(())
}

#[test]
fn pragma_former_id_fix_then_a_licence_leaves_the_tree_clean() -> Outcome {
    let project = renamed("danger(1)  # ast-grep-ignore: danger-old\n")?;
    project.config(&format!(
        "roots = [\"src\"]\ncatalogues = [\"cat/\"]\nexceptions = \"{EXCEPTIONS}\"\n"
    ))?;
    project.file(EXCEPTIONS, &licence("t.danger", "src/a.py", "pragma", None))?;
    let out = project.check(&["--fix"])?;
    assert_eq!(out.stdout, "Fixed 1 finding.\nAll checks passed.\n");
    assert_eq!(out.code, 0);
    Ok(())
}

#[test]
fn pragma_former_id_fix_is_safe_in_the_json_report() -> Outcome {
    let report = renamed("danger(1)  # ast-grep-ignore: danger-old\n")?.json(&[])?;
    let finding = &report["findings"]
        .as_array()
        .and_then(|all| all.iter().find(|finding| finding["rule"] == "law.pragma"))
        .ok_or("no pragma finding")?;
    assert_eq!(finding["fix"]["safety"], "safe");
    assert_eq!(finding["fix"]["edits"][0]["text"], "t.danger");
    Ok(())
}
