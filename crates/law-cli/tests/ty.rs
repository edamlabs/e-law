mod common;

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use common::{Out, Outcome, Project, RUFF, TY, only, passed, path_with, skip, stub, version_of};
use serde_json::Value;

const ANY: &str = "- id: t.ty
  message: type error
  check: {engine: ty}
  fix: {safety: none, reason: ty has no fixes}
";

const NAMED: &str = "- id: t.ret
  message: wrong return
  check: {engine: ty, rule: invalid-explicit-return}
  fix: {safety: none, reason: ty has no fixes}
";

const BAD: &str = "def f(x: int) -> str:\n    return 1\n";
const PRAGMA: &str = "law.pragma silenced inline: register an exception instead";

fn project(rules: &str) -> Result<Project, Box<dyn std::error::Error>> {
    Project::with_rules("[\"pkg\"]", rules)
}

fn licensed(project: &Project, rule: &str) -> Result<(), Box<dyn std::error::Error>> {
    project.config("roots = [\"pkg\"]\ncatalogues = [\"cat/\"]\nexceptions = \"ex.toml\"\n")?;
    project.file(
        "ex.toml",
        &format!(
            "[[exception]]\nrule = \"{rule}\"\npath = \"pkg/a.py\"\nreach = \"pragma\"\nreason = \"r\"\napproved = \"d\"\n"
        ),
    )?;
    Ok(())
}

fn stub_dir(project: &Project) -> PathBuf {
    project.path().join("stubs")
}

fn stub_path(project: &Project) -> Result<OsString, Box<dyn std::error::Error>> {
    only(&[&stub_dir(project), Path::new("/usr/bin"), Path::new("/bin")])
}

fn logging_stub(project: &Project, version: &str, stdout: &str) -> Outcome {
    let log = project.path().join("log");
    let script = format!(
        "echo \"ty $@\" >> {}\ncase \"$1\" in --version) echo \"ty {version}\";; *) cat <<'EOF'\n{stdout}\nEOF\n;; esac",
        log.display()
    );
    stub(&stub_dir(project), "ty", &script)?;
    Ok(())
}

fn sarif(uri: &str, rule: &str, line: u32, column: u32) -> String {
    format!(
        "{{\"runs\":[{{\"results\":[{{\"ruleId\":\"{rule}\",\"message\":{{\"text\":\"m\"}},\"locations\":[{{\"physicalLocation\":{{\"artifactLocation\":{{\"uri\":\"{uri}\"}},\"region\":{{\"startLine\":{line},\"startColumn\":{column},\"endLine\":{line},\"endColumn\":{}}}}}}}]}}]}}]}}",
        column + 1
    )
}

fn canonical(project: &Project) -> Result<String, Box<dyn std::error::Error>> {
    Ok(fs::canonicalize(project.path())?.display().to_string())
}

fn findings(out: &Out) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let value: Value = serde_json::from_str(&out.stdout)?;
    Ok(value["findings"].as_array().cloned().ok_or("no findings")?)
}

#[test]
fn ty_rule_finds_a_type_error_signed_with_its_own_id_and_the_tools_message() -> Outcome {
    let Some(path) = path_with(&[TY]) else { return skip(TY) };
    let project = project(ANY)?;
    project.file("pkg/a.py", BAD)?;
    let out = project.check_on(&path, &[])?;
    assert_eq!(
        out.stdout,
        "pkg/a.py:2:12: t.ty type error: Return type does not match returned value: expected `str`, found `Literal[1]`\n\nFound 1 finding (1 error, 0 warnings).\n"
    );
    assert_eq!((out.stderr.as_str(), out.code), ("", 1));
    Ok(())
}

#[test]
fn ty_syntax_error_in_a_file_is_a_fault_not_a_finding() -> Outcome {
    let Some(path) = path_with(&[TY]) else { return skip(TY) };
    let project = project(ANY)?;
    project.file("pkg/a.py", "x = (\n")?;
    let out = project.check_on(&path, &[])?;
    assert_eq!(out.code, 2, "{out:?}");
    assert_eq!(out.stdout, "1 fault: the run is incomplete.\n", "{out:?}");
    assert_eq!(
        out.stderr,
        "ty: pkg/a.py:2:1: invalid-syntax: unexpected EOF while parsing\n"
    );
    assert!(out.stderr.contains("invalid-syntax"), "{}", out.stderr);
    assert!(out.stderr.contains("pkg/a.py"), "{}", out.stderr);
    Ok(())
}

#[test]
fn ty_syntax_error_beside_a_type_error_still_faults_the_run() -> Outcome {
    let Some(path) = path_with(&[TY]) else { return skip(TY) };
    let project = project(ANY)?;
    project.file("pkg/a.py", "x = (\n")?;
    project.file("pkg/b.py", BAD)?;
    let out = project.check_on(&path, &[])?;
    assert_eq!(out.code, 2, "{out:?}");
    assert!(!out.stdout.contains("All checks passed"), "{out:?}");
    Ok(())
}

#[test]
fn ty_clean_project_passes() -> Outcome {
    let Some(path) = path_with(&[TY]) else { return skip(TY) };
    let project = project(ANY)?;
    project.file("pkg/a.py", "def f(x: int) -> int:\n    return x\n")?;
    let out = project.check_on(&path, &[])?;
    assert!(passed(&out), "{out:?}");
    Ok(())
}

#[test]
fn ty_columns_are_characters_and_spans_bytes_on_non_ascii_text() -> Outcome {
    let Some(path) = path_with(&[TY]) else { return skip(TY) };
    let project = project(ANY)?;
    let text = "def f() -> str:\n    s = \"caf\u{e9} \u{1f600}\"; return 1\n";
    project.file("pkg/a.py", text)?;
    let out = project.check_on(&path, &["--output-format", "json"])?;
    let found = findings(&out)?;
    assert_eq!(found.len(), 1, "{}", out.stdout);
    let line_start = text.find("    s").ok_or("line")?;
    let one = text.rfind('1').ok_or("1")?;
    let chars = text[line_start..one].chars().count() + 1;
    assert_eq!(found[0]["span"]["start"]["column"], chars);
    assert_eq!(found[0]["span"]["bytes"]["start"], one);
    assert!(text[line_start..one].len() > text[line_start..one].chars().count());
    Ok(())
}

#[test]
fn ty_named_rule_takes_its_own_diagnostics_only() -> Outcome {
    let Some(path) = path_with(&[TY]) else { return skip(TY) };
    let project = project(NAMED)?;
    project.file("pkg/a.py", "x: int = \"a\"\n\ndef f() -> str:\n    return 1\n")?;
    let out = project.check_on(&path, &[])?;
    assert!(
        out.stdout.starts_with("pkg/a.py:4:12: t.ret wrong return: "),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("Found 1 finding"), "{}", out.stdout);
    let other = project.check_on(&path, &["--output-format", "json"])?;
    assert_eq!(findings(&other)?.len(), 1);
    Ok(())
}

#[test]
fn ty_report_stamps_the_tool_version() -> Outcome {
    let Some(path) = path_with(&[TY]) else { return skip(TY) };
    let project = project(ANY)?;
    project.file("pkg/a.py", "x = 1\n")?;
    let out = project.check_on(&path, &["--output-format", "json"])?;
    let report: Value = serde_json::from_str(&out.stdout)?;
    assert_eq!(
        report["run"]["tools"],
        serde_json::json!([{"name": "ty", "version": version_of(TY)?}])
    );
    Ok(())
}

#[test]
fn ty_and_ruff_together_run_once_each_and_are_both_stamped() -> Outcome {
    let (Some(path), true) = (path_with(&[RUFF, TY]), true) else {
        return skip("ruff or ty");
    };
    let rules = format!(
        "{ANY}- id: t.print\n  message: no print\n  check: {{engine: ruff, code: T201}}\n  fix: {{safety: none, reason: ported}}\n"
    );
    let project = project(&rules)?;
    project.file("pkg/a.py", "def f() -> str:\n    print(1)\n    return 1\n")?;
    let out = project.check_on(&path, &["--output-format", "json"])?;
    let report: Value = serde_json::from_str(&out.stdout)?;
    let names: Vec<&str> = report["run"]["tools"]
        .as_array()
        .ok_or("tools")?
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert_eq!(names, ["ruff", "ty"]);
    let ids: Vec<&str> = findings(&out)?
        .iter()
        .filter_map(|f| f["rule"].as_str().map(|_| ""))
        .collect();
    assert_eq!(ids.len(), 2);
    Ok(())
}

#[test]
fn ty_unlicensed_ignore_comment_is_a_pragma_finding_and_licensed_passes() -> Outcome {
    let Some(path) = path_with(&[TY]) else { return skip(TY) };
    let project = project(ANY)?;
    project.file(
        "pkg/a.py",
        "def f() -> str:\n    return 1  # ty: ignore[invalid-explicit-return]\n",
    )?;
    let out = project.check_on(&path, &[])?;
    assert_eq!(
        out.stdout,
        format!(
            "pkg/a.py:2:15: {PRAGMA}: t.ty has no registered exception\n\nFound 1 finding (1 error, 0 warnings).\n"
        )
    );
    assert_eq!(out.code, 1);
    licensed(&project, "t.ty")?;
    let ok = project.check_on(&path, &[])?;
    assert!(passed(&ok), "{ok:?}");
    Ok(())
}

#[test]
fn ty_bare_ignore_comment_names_no_rule_and_no_licence_covers_it() -> Outcome {
    let Some(path) = path_with(&[TY]) else { return skip(TY) };
    for (comment, column) in [("# type: ignore", 15), ("# ty: ignore", 15)] {
        let project = project(ANY)?;
        project.file("pkg/a.py", &format!("def f() -> str:\n    return 1  {comment}\n"))?;
        let expected =
            format!("pkg/a.py:2:{column}: {PRAGMA}: names no rule\n\nFound 1 finding (1 error, 0 warnings).\n");
        assert_eq!(project.check_on(&path, &[])?.stdout, expected, "{comment}");
        licensed(&project, "t.ty")?;
        let out = project.check_on(&path, &[])?;
        assert!(
            out.stdout
                .contains(&expected.lines().next().map(str::to_owned).unwrap_or_default()),
            "{comment}, licensed: {out:?}"
        );
        assert!(
            out.stdout.contains("law.exception-unused"),
            "{comment}, licensed: the licence covers nothing: {out:?}"
        );
        assert_eq!(out.code, 1);
    }
    Ok(())
}

#[test]
fn ty_named_ignore_comment_needs_the_licence_of_its_rule() -> Outcome {
    let Some(path) = path_with(&[TY]) else { return skip(TY) };
    let named = project(NAMED)?;
    named.file("pkg/a.py", "x: int = \"a\"  # ty: ignore[invalid-assignment]\n")?;
    let out = named.check_on(&path, &[])?;
    assert!(passed(&out), "unnamed by the catalogue, left alone: {out:?}");
    let project = project(ANY)?;
    project.file("pkg/a.py", "x: int = \"a\"  # ty: ignore[invalid-assignment]\n")?;
    let out = project.check_on(&path, &[])?;
    assert_eq!(
        out.stdout,
        format!(
            "pkg/a.py:1:15: {PRAGMA}: t.ty has no registered exception\n\nFound 1 finding (1 error, 0 warnings).\n"
        )
    );
    licensed(&project, "t.ty")?;
    assert!(passed(&project.check_on(&path, &[])?));
    Ok(())
}

#[test]
fn ty_ignore_of_a_rule_the_catalogue_does_not_name_is_left_alone() -> Outcome {
    let Some(path) = path_with(&[TY]) else { return skip(TY) };
    let project = project(NAMED)?;
    project.file("pkg/a.py", "x: int = \"a\"  # ty: ignore[invalid-assignment]\n")?;
    let out = project.check_on(&path, &[])?;
    assert!(passed(&out), "{out:?}");
    Ok(())
}

#[test]
fn ty_ignore_comments_are_not_pragmas_when_no_ty_rule_is_loaded() -> Outcome {
    let project = project(common::ELIF)?;
    project.file("pkg/a.py", "x = 1  # ty: ignore[a]\ny = 2  # type: ignore\n")?;
    let out = project.check(&[])?;
    assert!(passed(&out), "{out:?}");
    Ok(())
}

#[test]
fn ty_trial_lists_the_rule_untested_with_its_why_and_never_starts_ty() -> Outcome {
    let rule = format!("{ANY}  cases:\n    invalid: [\"x: int = 'a'\\n\"]\n");
    let project = project(&rule)?;
    let empty = project.path().join("nothing");
    fs::create_dir_all(&empty)?;
    let out = project.run_on(&only(&[&empty])?, &["test"])?;
    assert_eq!(out.code, 0, "{out:?}");
    assert_eq!(
        out.stdout,
        "t.ty: untested: needs an environment\n\nTested 0 rules, 0 cases: 0 failed, 1 untested.\n"
    );
    Ok(())
}

#[test]
fn ty_one_process_per_run_with_exactly_the_contracted_arguments() -> Outcome {
    let project = project(ANY)?;
    project.file("pkg/a.py", "x = 1\n")?;
    project.file("pkg/b.py", "y = 2\n")?;
    logging_stub(&project, "9.9.9", "{\"runs\":[{\"results\":[]}]}")?;
    let out = project.check_on(&stub_path(&project)?, &[])?;
    assert!(passed(&out), "{out:?}");
    let log: Vec<String> = fs::read_to_string(project.path().join("log"))?
        .lines()
        .map(str::to_owned)
        .collect();
    assert_eq!(
        log,
        [
            "ty --version",
            "ty check --output-format sarif --exit-zero --no-progress -- pkg"
        ]
    );
    Ok(())
}

#[test]
fn ty_missing_while_needed_exits_2_naming_the_tool_and_the_rule() -> Outcome {
    let project = project(ANY)?;
    project.file("pkg/a.py", "x = 1\n")?;
    let empty = project.path().join("nothing");
    fs::create_dir_all(&empty)?;
    let out = project.check_on(&only(&[&empty])?, &[])?;
    assert_eq!(out.code, 2, "{out:?}");
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr
            .starts_with("ty: not found beside law, in .venv/bin nor on PATH; needed by `t.ty`"),
        "{}",
        out.stderr
    );
    Ok(())
}

#[test]
fn ty_abnormal_exit_is_a_fault_with_tys_own_message_not_zero_findings() -> Outcome {
    let Some(path) = path_with(&[TY]) else { return skip(TY) };
    let project = project(ANY)?;
    project.file(
        "pyproject.toml",
        "[tool.ty.environment]\npython = \"/nonexistent/python\"\n",
    )?;
    project.file("pkg/a.py", BAD)?;
    let out = project.check_on(&path, &[])?;
    assert_eq!(out.code, 2, "{out:?}");
    assert!(out.stderr.contains("ty: exited with 2: ty failed"), "{}", out.stderr);
    assert!(out.stderr.contains("environment.python"), "{}", out.stderr);
    assert!(!out.stdout.contains("All checks passed"), "{}", out.stdout);
    Ok(())
}

#[test]
fn ty_stdout_that_is_not_sarif_is_a_fault() -> Outcome {
    let project = project(ANY)?;
    project.file("pkg/a.py", "x = 1\n")?;
    logging_stub(&project, "9.9.9", "[]")?;
    let out = project.check_on(&stub_path(&project)?, &[])?;
    assert_eq!(out.code, 2, "{out:?}");
    assert!(
        out.stderr.contains("ty: its output is not the document expected"),
        "{}",
        out.stderr
    );
    Ok(())
}

#[test]
fn ty_result_outside_the_root_without_a_place_or_with_a_foreign_uri_is_a_fault() -> Outcome {
    let project = project(ANY)?;
    project.file("pkg/a.py", "x = 1\n")?;
    let root = canonical(&project)?;
    let cases = [
        (
            sarif("file:///elsewhere/x.py", "r", 1, 1),
            "ty: /elsewhere/x.py: a diagnostic outside the project root".to_owned(),
        ),
        (
            sarif("https://x/a.py", "r", 1, 1),
            "ty: its output is not the document expected: `https://x/a.py` is not a file URI".to_owned(),
        ),
        (
            sarif(&format!("file://{root}/pkg/a.py"), "r", 9, 1),
            "pkg/a.py:9:1: a place the file does not have".to_owned(),
        ),
        (
            "{\"runs\":[{\"results\":[{\"ruleId\":\"r\",\"message\":{\"text\":\"mm\"}}]}]}".to_owned(),
            "ty: `r` placed nowhere: mm".to_owned(),
        ),
    ];
    for (document, expected) in cases {
        logging_stub(&project, "9.9.9", &document)?;
        let out = project.check_on(&stub_path(&project)?, &[])?;
        assert_eq!(out.code, 2, "{expected}: {out:?}");
        assert!(out.stderr.contains(&expected), "{expected}: {}", out.stderr);
    }
    Ok(())
}

#[test]
fn ty_stub_result_is_signed_by_the_rule_with_the_composed_message() -> Outcome {
    let project = project(ANY)?;
    project.file("pkg/a.py", "x = 1\n")?;
    let root = canonical(&project)?;
    logging_stub(
        &project,
        "9.9.9",
        &sarif(&format!("file://{root}/pkg/a.py"), "anything", 1, 1),
    )?;
    let out = project.check_on(&stub_path(&project)?, &[])?;
    assert_eq!(
        out.stdout,
        "pkg/a.py:1:1: t.ty type error: m\n\nFound 1 finding (1 error, 0 warnings).\n"
    );
    Ok(())
}

#[test]
fn ty_stub_uri_with_percent_escapes_resolves_to_the_file() -> Outcome {
    let project = project(ANY)?;
    project.file("pkg/sp ace.py", "x = 1\n")?;
    let root = canonical(&project)?;
    logging_stub(
        &project,
        "9.9.9",
        &sarif(&format!("file://{root}/pkg/sp%20ace.py"), "anything", 1, 1),
    )?;
    let out = project.check_on(&stub_path(&project)?, &[])?;
    assert!(out.stdout.starts_with("pkg/sp ace.py:1:1: t.ty"), "{out:?}");
    Ok(())
}
