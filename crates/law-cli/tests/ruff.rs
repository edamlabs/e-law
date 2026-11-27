mod common;

use std::fs;
use std::path::Path;
use std::process::Command;

use common::{Out, Outcome, Project, RUFF, only, passed, path_with, skip, stub, tool_dir, version_of};
use serde_json::Value;

const PRINT: &str = "- id: t.print
  message: no print
  scope: {include: [\"pkg/**/*.py\"]}
  check: {engine: ruff, code: T201}
  fix: {safety: safe}
";

const IMPORTS: &str = "- id: t.unused
  message: unused import
  scope: {include: [\"pkg/**/*.py\"]}
  check: {engine: ruff, code: F401}
  fix: {safety: safe}
";

const BARE: &str = "- id: t.bare
  message: bare except
  scope: {include: [\"pkg/**/*.py\"]}
  check: {engine: ruff, code: E722}
  fix: {safety: none, reason: ported}
";

const PRAGMA: &str = "law.pragma silenced inline: register an exception instead";
const EXCEPTIONS: &str = "ex.toml";

fn project(rules: &str) -> Result<Project, Box<dyn std::error::Error>> {
    Project::with_rules("[\"pkg\"]", rules)
}

fn licences(project: &Project, rules: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    project.config("roots = [\"pkg\"]\ncatalogues = [\"cat/\"]\nexceptions = \"ex.toml\"\n")?;
    let text = rules
        .iter()
        .map(|rule| {
            format!(
                "[[exception]]\nrule = \"{rule}\"\npath = \"pkg/a.py\"\nreach = \"pragma\"\nreason = \"r\"\napproved = \"d\"\n\n"
            )
        })
        .collect::<Vec<String>>()
        .concat();
    project.file(EXCEPTIONS, &text)?;
    Ok(())
}

fn real(tools: &[&str]) -> Option<std::ffi::OsString> {
    path_with(tools)
}

fn findings(out: &Out) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let value: Value = serde_json::from_str(&out.stdout)?;
    Ok(value["findings"].as_array().cloned().ok_or("no findings")?)
}

fn args_log(project: &Project) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    Ok(fs::read_to_string(project.path().join("log"))?
        .lines()
        .map(str::to_owned)
        .collect())
}

fn stub_dir(project: &Project) -> std::path::PathBuf {
    project.path().join("stubs")
}

fn logging_stub(project: &Project, tool: &str, version: &str, stdout: &str) -> Outcome {
    let log = project.path().join("log");
    let script = format!(
        "echo \"{tool} $@\" >> {}\ncase \"$1\" in --version) echo \"{tool} {version}\";; *) cat <<'EOF'\n{stdout}\nEOF\n;; esac",
        log.display()
    );
    stub(&stub_dir(project), tool, &script)?;
    Ok(())
}

fn stub_path(project: &Project) -> Result<std::ffi::OsString, Box<dyn std::error::Error>> {
    only(&[&stub_dir(project), Path::new("/usr/bin"), Path::new("/bin")])
}

fn canonical(project: &Project) -> Result<String, Box<dyn std::error::Error>> {
    Ok(fs::canonicalize(project.path())?.display().to_string())
}

fn diagnostic(file: &str, code: &str, row: u32, column: u32) -> String {
    format!(
        "[{{\"code\":\"{code}\",\"filename\":\"{file}\",\"location\":{{\"row\":{row},\"column\":{column}}},\"end_location\":{{\"row\":{row},\"column\":{}}},\"message\":\"m\",\"fix\":null}}]",
        column + 1
    )
}

#[test]
fn ruff_rule_finds_print_signed_with_its_own_id_and_the_tools_message() -> Outcome {
    let Some(path) = real(&[RUFF]) else { return skip(RUFF) };
    let project = project(PRINT)?;
    project.file("pkg/a.py", "x = 1\nprint(x)\n")?;
    let out = project.check_on(&path, &[])?;
    assert_eq!(
        out.stdout,
        "pkg/a.py:2:1: t.print no print: `print` found\n\nFound 1 finding (1 error, 0 warnings).\n"
    );
    assert_eq!((out.stderr.as_str(), out.code), ("", 1));
    Ok(())
}

#[test]
fn ruff_clean_file_passes() -> Outcome {
    let Some(path) = real(&[RUFF]) else { return skip(RUFF) };
    let project = project(PRINT)?;
    project.file("pkg/a.py", "x = 1\n")?;
    let out = project.check_on(&path, &[])?;
    assert!(passed(&out), "{out:?}");
    Ok(())
}

#[test]
fn ruff_report_stamps_the_tool_version() -> Outcome {
    let Some(path) = real(&[RUFF]) else { return skip(RUFF) };
    let project = project(PRINT)?;
    project.file("pkg/a.py", "x = 1\n")?;
    let out = project.check_on(&path, &["--output-format", "json"])?;
    let report: Value = serde_json::from_str(&out.stdout)?;
    let tools = report["run"]["tools"].as_array().ok_or("no tools")?;
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["name"], "ruff");
    assert_eq!(tools[0]["version"], version_of(RUFF)?);
    Ok(())
}

#[test]
fn ruff_columns_are_characters_and_spans_are_bytes_on_non_ascii_text() -> Outcome {
    let Some(path) = real(&[RUFF]) else { return skip(RUFF) };
    let project = project(PRINT)?;
    let text = "s = \"caf\u{e9} \u{1f600} \u{4e16}\"; print(s)\n";
    project.file("pkg/a.py", text)?;
    let out = project.check_on(&path, &["--output-format", "json"])?;
    let found = findings(&out)?;
    let before = &text[..text.find("print").ok_or("no print")?];
    assert_eq!(found[0]["span"]["start"]["column"], before.chars().count() + 1);
    assert_eq!(found[0]["span"]["bytes"]["start"], before.len());
    assert_eq!(found[0]["span"]["bytes"]["end"], before.len() + "print".len());
    assert!(
        before.len() > before.chars().count(),
        "the line has multi-byte characters"
    );
    Ok(())
}

#[test]
fn ruff_rows_follow_crlf_and_lone_cr_line_breaks() -> Outcome {
    let Some(path) = real(&[RUFF]) else { return skip(RUFF) };
    for (name, text) in [("crlf", "a = 1\r\nprint(a)\r\n"), ("cr", "a = 1\rprint(a)\r")] {
        let project = project(PRINT)?;
        project.file("pkg/a.py", text)?;
        let out = project.check_on(&path, &["--output-format", "json"])?;
        let found = findings(&out)?;
        assert_eq!(found.len(), 1, "{name}: {}", out.stdout);
        assert_eq!(found[0]["span"]["start"]["line"], 2, "{name}");
        assert_eq!(
            found[0]["span"]["bytes"]["start"],
            6 + usize::from(name == "crlf"),
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn ruff_fix_through_our_fixer_equals_ruff_check_fix_byte_for_byte() -> Outcome {
    let Some(path) = real(&[RUFF]) else { return skip(RUFF) };
    let text =
        "import os\r\nimport sys\r\ns = \"caf\u{e9} \u{1f600}\"\r\nprint(s)\r\nt = \"\u{4e16}\u{754c}\"; print(t)\r\n";
    let project = project(&format!("{PRINT}{IMPORTS}"))?;
    project.file("pkg/a.py", text)?;
    let theirs = Project::new()?;
    let copy = theirs.file("pkg/a.py", text)?;
    let dir = tool_dir(RUFF).ok_or("ruff")?;
    let ran = Command::new(dir.join(RUFF))
        .current_dir(theirs.path())
        .args([
            "check",
            "--isolated",
            "--select",
            "T201,F401",
            "--fix",
            "--unsafe-fixes",
            "--exit-zero",
            "pkg",
        ])
        .output()?;
    assert!(ran.status.success(), "{}", String::from_utf8_lossy(&ran.stderr));
    let out = project.check_on(&path, &["--fix", "--unsafe-fixes"])?;
    assert_eq!(out.code, 0, "{out:?}");
    assert!(out.stdout.starts_with("Fixed "), "{}", out.stdout);
    assert_eq!(fs::read(project.path().join("pkg/a.py"))?, fs::read(copy)?);
    assert_ne!(fs::read(project.path().join("pkg/a.py"))?, text.as_bytes());
    Ok(())
}

#[test]
fn ruff_fix_safe_rule_applies_a_safe_fix_and_an_unsafe_one_only_with_the_flag() -> Outcome {
    let Some(path) = real(&[RUFF]) else { return skip(RUFF) };
    let project = project(&format!("{PRINT}{IMPORTS}"))?;
    project.file("pkg/a.py", "import os\nprint(1)\n")?;
    let kept = project.check_on(&path, &["--fix"])?;
    assert_eq!(kept.code, 1, "{kept:?}");
    assert_eq!(
        fs::read_to_string(project.path().join("pkg/a.py"))?,
        "print(1)\n",
        "F401 is safe in ruff and under the rule: applied; T201 is unsafe in ruff: left"
    );
    let taken = project.check_on(&path, &["--fix", "--unsafe-fixes"])?;
    assert_eq!(taken.code, 0, "{taken:?}");
    assert_eq!(fs::read_to_string(project.path().join("pkg/a.py"))?, "");
    Ok(())
}

#[test]
fn ruff_fix_policy_none_never_touches_the_file_though_ruff_offers_one() -> Outcome {
    let Some(path) = real(&[RUFF]) else { return skip(RUFF) };
    let rules = IMPORTS.replace("fix: {safety: safe}", "fix: {safety: none, reason: ported}");
    let project = project(&rules)?;
    project.file("pkg/a.py", "import os\n")?;
    let out = project.check_on(&path, &["--fix", "--unsafe-fixes"])?;
    assert_eq!(out.code, 1, "{out:?}");
    assert_eq!(fs::read_to_string(project.path().join("pkg/a.py"))?, "import os\n");
    let found = findings(&project.check_on(&path, &["--output-format", "json"])?)?;
    assert!(found[0]["fix"].is_null(), "{found:?}");
    Ok(())
}

#[test]
fn ruff_policy_unsafe_downgrades_a_safe_ruff_fix() -> Outcome {
    let Some(path) = real(&[RUFF]) else { return skip(RUFF) };
    let project = project(&IMPORTS.replace("safety: safe", "safety: unsafe"))?;
    project.file("pkg/a.py", "import os\n")?;
    let found = findings(&project.check_on(&path, &["--output-format", "json"])?)?;
    assert_eq!(found[0]["fix"]["safety"], "unsafe");
    assert_eq!(project.check_on(&path, &["--fix"])?.code, 1);
    assert_eq!(project.check_on(&path, &["--fix", "--unsafe-fixes"])?.code, 0);
    Ok(())
}

#[test]
fn ruff_split_of_one_code_by_message_routes_each_diagnostic_to_its_rule() -> Outcome {
    let Some(path) = real(&[RUFF]) else { return skip(RUFF) };
    let rules = "- id: t.environ
  message: no environ
  check: {engine: ruff, code: TID251, message: \"os.environ\"}
  fix: {safety: none, reason: ported}
- id: t.getenv
  message: no getenv
  check: {engine: ruff, code: TID251, message: \"os.getenv\"}
  fix: {safety: none, reason: ported}
";
    let project = project(rules)?;
    project.file(
        "pyproject.toml",
        "[tool.ruff.lint.flake8-tidy-imports.banned-api]\n\"os.environ\".msg = \"use settings\"\n\"os.getenv\".msg = \"use settings\"\n",
    )?;
    project.file("pkg/a.py", "import os\n\na = os.environ\nb = os.getenv('x')\n")?;
    let out = project.check_on(&path, &[])?;
    assert!(
        out.stdout
            .contains("pkg/a.py:3:5: t.environ no environ: `os.environ` is banned"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout
            .contains("pkg/a.py:4:5: t.getenv no getenv: `os.getenv` is banned"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("Found 2 findings"), "{}", out.stdout);
    Ok(())
}

#[test]
fn ruff_select_narrows_the_findings_to_the_selected_rule() -> Outcome {
    let Some(path) = real(&[RUFF]) else { return skip(RUFF) };
    let project = project(&format!("{PRINT}{BARE}"))?;
    project.file("pkg/a.py", "try:\n    print(1)\nexcept:\n    pass\n")?;
    let both = project.check_on(&path, &[])?;
    assert!(
        both.stdout.contains("t.print") && both.stdout.contains("t.bare"),
        "{}",
        both.stdout
    );
    let only_print = project.check_on(&path, &["--select", "t.print"])?;
    assert!(
        only_print.stdout.contains("t.print") && !only_print.stdout.contains("t.bare"),
        "{}",
        only_print.stdout
    );
    Ok(())
}

#[test]
fn ruff_broken_project_config_is_a_fault_not_zero_findings() -> Outcome {
    let Some(path) = real(&[RUFF]) else { return skip(RUFF) };
    for (name, text) in [
        ("pyproject.toml", "[tool.ruff]\nline-length = \"x\"\n"),
        ("ruff.toml", "line-length = \"x\"\n"),
    ] {
        let project = project(PRINT)?;
        project.file(name, text)?;
        project.file("pkg/a.py", "print(1)\n")?;
        let out = project.check_on(&path, &[])?;
        assert_eq!(out.code, 2, "{name}: {out:?}");
        assert!(out.stderr.contains("ruff"), "{name}: {}", out.stderr);
        assert!(!out.stdout.contains("All checks passed"), "{name}: {}", out.stdout);
    }
    Ok(())
}

#[test]
fn ruff_syntax_error_in_a_file_is_a_fault_not_silence() -> Outcome {
    let Some(path) = real(&[RUFF]) else { return skip(RUFF) };
    let project = project(PRINT)?;
    project.file("pkg/a.py", "x = (\n")?;
    let out = project.check_on(&path, &[])?;
    assert_eq!(out.code, 2, "{out:?}");
    assert!(out.stderr.contains("invalid-syntax"), "{}", out.stderr);
    assert!(out.stderr.contains("pkg/a.py"), "{}", out.stderr);
    assert!(!out.stdout.contains("All checks passed"), "{}", out.stdout);
    Ok(())
}

#[test]
fn ruff_syntax_error_beside_a_good_file_still_reports_the_findings() -> Outcome {
    let Some(path) = real(&[RUFF]) else { return skip(RUFF) };
    let project = project(PRINT)?;
    project.file("pkg/a.py", "x = (\n")?;
    project.file("pkg/b.py", "print(1)\n")?;
    let out = project.check_on(&path, &[])?;
    assert_eq!(out.code, 2, "{out:?}");
    assert!(out.stdout.contains("pkg/b.py:1:1: t.print"), "{}", out.stdout);
    Ok(())
}

#[test]
fn ruff_noqa_unlicensed_is_a_pragma_finding_and_licensed_passes() -> Outcome {
    let Some(path) = real(&[RUFF]) else { return skip(RUFF) };
    let project = project(PRINT)?;
    project.file("pkg/a.py", "print(1)  # noqa: T201\n")?;
    let out = project.check_on(&path, &[])?;
    assert_eq!(
        out.stdout,
        format!(
            "pkg/a.py:1:11: {PRAGMA}: t.print has no registered exception\n\nFound 1 finding (1 error, 0 warnings).\n"
        )
    );
    assert_eq!(out.code, 1);
    licences(&project, &["t.print"])?;
    let licensed = project.check_on(&path, &[])?;
    assert!(passed(&licensed), "{licensed:?}");
    Ok(())
}

#[test]
fn ruff_noqa_licence_of_another_path_or_reach_does_not_license() -> Outcome {
    let Some(path) = real(&[RUFF]) else { return skip(RUFF) };
    let project = project(PRINT)?;
    project.file("pkg/b.py", "print(1)  # noqa: T201\n")?;
    licences(&project, &["t.print"])?;
    let out = project.check_on(&path, &[])?;
    assert!(out.stdout.contains("pkg/b.py:1:11"), "{}", out.stdout);
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn ruff_bare_noqa_names_no_rule_and_so_does_a_file_level_one() -> Outcome {
    let Some(path) = real(&[RUFF]) else { return skip(RUFF) };
    for (comment, position) in [("print(1)  # noqa\n", "1:11"), ("# ruff: noqa\nprint(1)\n", "1:1")] {
        let project = project(PRINT)?;
        project.file("pkg/a.py", comment)?;
        let out = project.check_on(&path, &[])?;
        assert_eq!(
            out.stdout,
            format!("pkg/a.py:{position}: {PRAGMA}: names no rule\n\nFound 1 finding (1 error, 0 warnings).\n"),
            "{comment:?}"
        );
    }
    Ok(())
}

#[test]
fn ruff_noqa_for_a_code_outside_the_catalogue_is_left_alone() -> Outcome {
    let Some(path) = real(&[RUFF]) else { return skip(RUFF) };
    let project = project(PRINT)?;
    project.file("pkg/a.py", "x = 1  # noqa: E501\n")?;
    let out = project.check_on(&path, &[])?;
    assert!(passed(&out), "{out:?}");
    Ok(())
}

#[test]
fn ruff_noqa_naming_a_catalogue_code_and_a_foreign_one_reports_only_ours() -> Outcome {
    let Some(path) = real(&[RUFF]) else { return skip(RUFF) };
    let project = project(PRINT)?;
    project.file("pkg/a.py", "print(1)  # noqa: E501, T201\n")?;
    let out = project.check_on(&path, &[])?;
    assert_eq!(
        out.stdout,
        format!(
            "pkg/a.py:1:11: {PRAGMA}: t.print has no registered exception\n\nFound 1 finding (1 error, 0 warnings).\n"
        )
    );
    Ok(())
}

#[test]
fn ruff_noqa_one_licence_per_rule_naming_the_code() -> Outcome {
    let Some(path) = real(&[RUFF]) else { return skip(RUFF) };
    let two = "- id: t.one
  message: one
  check: {engine: ruff, code: TID251, message: \"os.environ\"}
  fix: {safety: none, reason: ported}
- id: t.two
  message: two
  check: {engine: ruff, code: TID251, message: \"os.getenv\"}
  fix: {safety: none, reason: ported}
";
    let project = project(two)?;
    project.file("pkg/a.py", "import os\n\na = os.environ  # noqa: TID251\n")?;
    licences(&project, &["t.one"])?;
    let partly = project.check_on(&path, &[])?;
    assert_eq!(partly.code, 1, "{partly:?}");
    assert!(
        partly.stdout.contains("t.two has no registered exception"),
        "{}",
        partly.stdout
    );
    assert!(!partly.stdout.contains("t.one has no registered"), "{}", partly.stdout);
    licences(&project, &["t.one", "t.two"])?;
    assert!(passed(&project.check_on(&path, &[])?));
    Ok(())
}

#[test]
fn ruff_bare_noqa_is_ignored_when_no_ruff_rule_is_loaded() -> Outcome {
    let native = common::ELIF;
    let project = project(native)?;
    project.file("pkg/a.py", "x = 1  # noqa\n")?;
    let out = project.check(&[])?;
    assert!(passed(&out), "{out:?}");
    Ok(())
}

#[test]
fn ruff_trial_proves_valid_invalid_and_fixed_cases_through_ruff() -> Outcome {
    let Some(path) = real(&[RUFF]) else { return skip(RUFF) };
    let rule = "- id: t.unused
  message: unused import
  scope: {include: [\"pkg/**/*.py\"]}
  check: {engine: ruff, code: F401}
  fix: {safety: safe}
  cases:
    valid: [\"import os\\nos.getcwd()\\n\"]
    invalid: [\"import os\\n\"]
    fixed: [{code: \"import os\\n\", expect: \"\"}]
";
    let project = project(rule)?;
    let out = project.run_on(&path, &["test"])?;
    assert_eq!(out.stdout, "Tested 1 rule, 3 cases: 0 failed, 0 untested.\n");
    assert_eq!(out.code, 0, "{out:?}");
    Ok(())
}

#[test]
fn ruff_trial_failing_cases_are_reported() -> Outcome {
    let Some(path) = real(&[RUFF]) else { return skip(RUFF) };
    let rule = "- id: t.unused
  message: unused import
  scope: {include: [\"pkg/**/*.py\"]}
  check: {engine: ruff, code: F401}
  fix: {safety: safe}
  cases:
    valid: [\"import os\\n\"]
    invalid: [\"x = 1\\n\"]
    fixed: [{code: \"import os\\n\", expect: \"import os\\n\"}]
";
    let project = project(rule)?;
    let out = project.run_on(&path, &["test"])?;
    assert_eq!(out.code, 1, "{out:?}");
    assert!(
        out.stdout
            .contains("t.unused: valid case 1: expected no finding, found 1"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("t.unused: invalid case 1"), "{}", out.stdout);
    assert!(
        out.stdout.contains("0 untested") && out.stdout.contains("3 failed"),
        "{}",
        out.stdout
    );
    Ok(())
}

#[test]
fn ruff_trial_runs_with_the_projects_ruff_configuration() -> Outcome {
    let Some(path) = real(&[RUFF]) else { return skip(RUFF) };
    let rule = "- id: t.environ
  message: no environ
  scope: {include: [\"pkg/**/*.py\"]}
  check: {engine: ruff, code: TID251}
  fix: {safety: none, reason: ported}
  cases:
    invalid: [\"import os\\n\\na = os.environ\\n\"]
";
    let with = project(rule)?;
    with.file(
        "pyproject.toml",
        "[tool.ruff.lint.flake8-tidy-imports.banned-api]\n\"os.environ\".msg = \"use settings\"\n",
    )?;
    let ok = with.run_on(&path, &["test"])?;
    assert_eq!(ok.stdout, "Tested 1 rule, 1 case: 0 failed, 0 untested.\n", "{ok:?}");
    let without = project(rule)?;
    let bad = without.run_on(&path, &["test"])?;
    assert_eq!(
        bad.code, 1,
        "without the banned-api configuration the case cannot be caught: {bad:?}"
    );
    Ok(())
}

#[test]
fn ruff_one_process_per_run_with_exactly_the_contracted_arguments() -> Outcome {
    let project = project(&format!("{PRINT}{BARE}"))?;
    project.file("pkg/a.py", "x = 1\n")?;
    project.file("pkg/b.py", "y = 2\n")?;
    logging_stub(&project, "ruff", "9.9.9", "[]")?;
    let out = project.check_on(&stub_path(&project)?, &[])?;
    assert!(passed(&out), "{out:?}");
    let log = args_log(&project)?;
    assert_eq!(
        log,
        [
            "ruff --version",
            "ruff check --output-format json --no-fix --exit-zero --select E722,T201 -- pkg"
        ]
    );
    Ok(())
}

#[test]
fn ruff_select_narrows_the_codes_passed_to_ruff() -> Outcome {
    let project = project(&format!("{PRINT}{BARE}"))?;
    project.file("pkg/a.py", "x = 1\n")?;
    logging_stub(&project, "ruff", "9.9.9", "[]")?;
    let out = project.check_on(&stub_path(&project)?, &["--select", "t.print"])?;
    assert!(passed(&out), "{out:?}");
    assert_eq!(
        args_log(&project)?,
        [
            "ruff --version",
            "ruff check --output-format json --no-fix --exit-zero --select T201 -- pkg"
        ]
    );
    Ok(())
}

#[test]
fn ruff_not_started_when_no_ruff_rule_is_selected() -> Outcome {
    let project = project(&format!("{PRINT}{}", common::ELIF.replace("t.elif", "t.native")))?;
    project.file("pkg/a.py", "x = 1\n")?;
    logging_stub(&project, "ruff", "9.9.9", "[]")?;
    let out = project.check_on(&stub_path(&project)?, &["--select", "t.native"])?;
    assert!(passed(&out), "{out:?}");
    assert!(
        !project.path().join("log").exists(),
        "ruff must not even be asked its version"
    );
    Ok(())
}

#[test]
fn ruff_the_stamp_names_each_tool_run_and_only_those() -> Outcome {
    let project = project(&format!("{PRINT}{}", common::ELIF.replace("t.elif", "t.native")))?;
    project.file("pkg/a.py", "x = 1\n")?;
    logging_stub(&project, "ruff", "9.9.9", "[]")?;
    let out = project.check_on(&stub_path(&project)?, &["--output-format", "json"])?;
    let report: Value = serde_json::from_str(&out.stdout)?;
    assert_eq!(
        report["run"]["tools"],
        serde_json::json!([{"name": "ruff", "version": "9.9.9"}])
    );
    let none = project.check_on(
        &stub_path(&project)?,
        &["--output-format", "json", "--select", "t.native"],
    )?;
    let report: Value = serde_json::from_str(&none.stdout)?;
    assert!(
        report["run"]["tools"].as_array().is_none_or(Vec::is_empty),
        "{}",
        none.stdout
    );
    Ok(())
}

#[test]
fn ruff_missing_while_needed_exits_2_naming_the_tool_and_the_rules() -> Outcome {
    let project = project(&format!("{PRINT}{BARE}"))?;
    project.file("pkg/a.py", "print(1)\n")?;
    let empty = project.path().join("nothing");
    fs::create_dir_all(&empty)?;
    let out = project.check_on(&only(&[&empty])?, &[])?;
    assert_eq!(out.code, 2, "{out:?}");
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr
            .starts_with("ruff: not found beside law, in .venv/bin nor on PATH; needed by "),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("`t.print`") && out.stderr.contains("`t.bare`"),
        "{}",
        out.stderr
    );
    Ok(())
}

#[test]
fn ruff_missing_but_not_selected_is_no_fault() -> Outcome {
    let project = project(&format!("{PRINT}{}", common::ELIF.replace("t.elif", "t.native")))?;
    project.file("pkg/a.py", "x = 1\n")?;
    let empty = project.path().join("nothing");
    fs::create_dir_all(&empty)?;
    let out = project.check_on(&only(&[&empty])?, &["--select", "t.native"])?;
    assert!(passed(&out), "{out:?}");
    Ok(())
}

#[test]
fn ruff_missing_names_only_the_selected_rules() -> Outcome {
    let project = project(&format!("{PRINT}{BARE}"))?;
    project.file("pkg/a.py", "x = 1\n")?;
    let empty = project.path().join("nothing");
    fs::create_dir_all(&empty)?;
    let out = project.check_on(&only(&[&empty])?, &["--select", "t.bare"])?;
    assert_eq!(out.code, 2, "{out:?}");
    assert!(
        out.stderr.contains("`t.bare`") && !out.stderr.contains("`t.print`"),
        "{}",
        out.stderr
    );
    Ok(())
}

#[test]
fn ruff_is_found_in_the_projects_venv_before_path() -> Outcome {
    let project = project(PRINT)?;
    project.file("pkg/a.py", "x = 1\n")?;
    stub(
        &project.path().join(".venv/bin"),
        "ruff",
        "case \"$1\" in --version) echo 'ruff 8.8.8';; *) echo '[]';; esac",
    )?;
    stub(
        &stub_dir(&project),
        "ruff",
        "case \"$1\" in --version) echo 'ruff 7.7.7';; *) echo '[]';; esac",
    )?;
    let out = project.check_on(&stub_path(&project)?, &["--output-format", "json"])?;
    let report: Value = serde_json::from_str(&out.stdout)?;
    assert_eq!(report["run"]["tools"][0]["version"], "8.8.8", "{}", out.stdout);
    Ok(())
}

#[test]
fn ruff_is_found_beside_the_law_executable_before_venv_and_path() -> Outcome {
    let project = project(PRINT)?;
    project.file("pkg/a.py", "x = 1\n")?;
    let bin = project.path().join("beside");
    fs::create_dir_all(&bin)?;
    let law = bin.join("law");
    fs::copy(env!("CARGO_BIN_EXE_law"), &law)?;
    stub(
        &bin,
        "ruff",
        "case \"$1\" in --version) echo 'ruff 6.6.6';; *) echo '[]';; esac",
    )?;
    stub(
        &project.path().join(".venv/bin"),
        "ruff",
        "case \"$1\" in --version) echo 'ruff 8.8.8';; *) echo '[]';; esac",
    )?;
    let out = common::output(
        Command::new(&law)
            .current_dir(project.path())
            .env("PATH", stub_path(&project)?)
            .args(["check", "--output-format", "json"]),
    )?;
    let report: Value = serde_json::from_slice(&out.stdout)?;
    assert_eq!(
        report["run"]["tools"][0]["version"],
        "6.6.6",
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    Ok(())
}

#[test]
fn ruff_exit_status_other_than_success_is_a_fault_with_stderrs_lines() -> Outcome {
    let project = project(PRINT)?;
    project.file("pkg/a.py", "print(1)\n")?;
    stub(
        &stub_dir(&project),
        "ruff",
        "case \"$1\" in --version) echo 'ruff 9.9.9';; *) echo 'boom one' >&2; echo 'boom two' >&2; exit 3;; esac",
    )?;
    let out = project.check_on(&stub_path(&project)?, &[])?;
    assert_eq!(out.code, 2, "{out:?}");
    assert!(
        out.stderr.contains("ruff: exited with 3: boom one / boom two"),
        "{}",
        out.stderr
    );
    assert!(!out.stdout.contains("All checks passed"), "{}", out.stdout);
    Ok(())
}

#[test]
fn ruff_stdout_that_is_not_the_document_is_a_fault() -> Outcome {
    let project = project(PRINT)?;
    project.file("pkg/a.py", "print(1)\n")?;
    logging_stub(&project, "ruff", "9.9.9", "this is not json")?;
    let out = project.check_on(&stub_path(&project)?, &[])?;
    assert_eq!(out.code, 2, "{out:?}");
    assert!(
        out.stderr.contains("ruff: its output is not the document expected"),
        "{}",
        out.stderr
    );
    Ok(())
}

#[test]
fn ruff_diagnostic_outside_the_project_root_is_a_fault() -> Outcome {
    let project = project(PRINT)?;
    project.file("pkg/a.py", "print(1)\n")?;
    logging_stub(&project, "ruff", "9.9.9", &diagnostic("/elsewhere/x.py", "T201", 1, 1))?;
    let out = project.check_on(&stub_path(&project)?, &[])?;
    assert_eq!(out.code, 2, "{out:?}");
    assert!(
        out.stderr
            .contains("ruff: /elsewhere/x.py: a diagnostic outside the project root"),
        "{}",
        out.stderr
    );
    Ok(())
}

#[test]
fn ruff_diagnostic_at_a_place_the_file_does_not_have_is_a_fault() -> Outcome {
    let project = project(PRINT)?;
    project.file("pkg/a.py", "print(1)\n")?;
    let file = format!("{}/pkg/a.py", canonical(&project)?);
    logging_stub(&project, "ruff", "9.9.9", &diagnostic(&file, "T201", 40, 1))?;
    let out = project.check_on(&stub_path(&project)?, &[])?;
    assert_eq!(out.code, 2, "{out:?}");
    assert!(
        out.stderr.contains("pkg/a.py:40:1: a place the file does not have"),
        "{}",
        out.stderr
    );
    Ok(())
}

#[test]
fn ruff_diagnostic_of_a_code_nobody_asked_for_is_a_fault() -> Outcome {
    let project = project(PRINT)?;
    project.file("pkg/a.py", "print(1)\n")?;
    let file = format!("{}/pkg/a.py", canonical(&project)?);
    logging_stub(&project, "ruff", "9.9.9", &diagnostic(&file, "E999", 1, 1))?;
    let out = project.check_on(&stub_path(&project)?, &[])?;
    assert_eq!(out.code, 2, "{out:?}");
    assert!(out.stderr.contains("pkg/a.py:1:1: E999: m"), "{}", out.stderr);
    Ok(())
}

#[test]
fn ruff_stub_diagnostic_is_signed_by_the_rule_with_the_composed_message() -> Outcome {
    let project = project(PRINT)?;
    project.file("pkg/a.py", "print(1)\n")?;
    let file = format!("{}/pkg/a.py", canonical(&project)?);
    logging_stub(&project, "ruff", "9.9.9", &diagnostic(&file, "T201", 1, 1))?;
    let out = project.check_on(&stub_path(&project)?, &[])?;
    assert_eq!(
        out.stdout,
        "pkg/a.py:1:1: t.print no print: m\n\nFound 1 finding (1 error, 0 warnings).\n"
    );
    Ok(())
}

#[test]
fn ruff_findings_outside_the_rules_scope_are_filtered_like_everyones() -> Outcome {
    let rule = PRINT.replace("pkg/**/*.py", "pkg/keep/**/*.py");
    let Some(path) = real(&[RUFF]) else { return skip(RUFF) };
    let project = project(&rule)?;
    project.file("pkg/keep/a.py", "print(1)\n")?;
    project.file("pkg/skip/b.py", "print(2)\n")?;
    let out = project.check_on(&path, &[])?;
    assert_eq!(
        out.stdout,
        "pkg/keep/a.py:1:1: t.print no print: `print` found\n\nFound 1 finding (1 error, 0 warnings).\n"
    );
    Ok(())
}

#[test]
fn ruff_exception_by_path_silences_its_findings_like_any_rules() -> Outcome {
    let Some(path) = real(&[RUFF]) else { return skip(RUFF) };
    let project = project(PRINT)?;
    project.config("roots = [\"pkg\"]\ncatalogues = [\"cat/\"]\nexceptions = \"ex.toml\"\n")?;
    project.file(
        "ex.toml",
        "[[exception]]\nrule = \"t.print\"\npath = \"pkg/a.py\"\nreason = \"r\"\napproved = \"d\"\n",
    )?;
    project.file("pkg/a.py", "print(1)\n")?;
    project.file("pkg/b.py", "print(2)\n")?;
    let out = project.check_on(&path, &[])?;
    assert_eq!(
        out.stdout,
        "pkg/b.py:1:1: t.print no print: `print` found\n\nFound 1 finding (1 error, 0 warnings).\n"
    );
    Ok(())
}
