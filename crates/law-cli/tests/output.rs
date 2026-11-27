mod common;

use common::{ELIF, Outcome, Project, WITH_ELIF, passed};
use std::process::{Command, Stdio};

use serde_json::json;

const GOLDEN_DIGEST: &str = "sha256:f3bdeda0fdfd850c5b0f6039d3dd364ed5a6e4284868c0e2f15882116b56391e";

fn digest_of(project: &Project) -> Result<String, Box<dyn std::error::Error>> {
    let report = project.json(&[])?;
    Ok(report["run"]["catalogues"][0]["digest"]
        .as_str()
        .ok_or("no digest")?
        .to_owned())
}

fn warn(rules: &str) -> String {
    rules
        .replace("t.elif", "t.warn")
        .replace("fix: {safety: none", "severity: warning\n  fix: {safety: none")
}

fn without_date(text: &str) -> String {
    text.lines()
        .filter(|line| !line.contains("\"date\":"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn output_text_clean_is_one_line() -> Outcome {
    let project = Project::with_rules("[\"src\"]", ELIF)?;
    project.file("src/a.py", "x = 1\n")?;
    assert!(passed(&project.check(&[])?));
    Ok(())
}

#[test]
fn output_text_summary_counts_errors_and_warnings() -> Outcome {
    let both = format!("{ELIF}{}", warn(ELIF).replace("elif_clause", "lambda"));
    let project = Project::with_rules("[\"src\"]", &both)?;
    project.file("src/a.py", &format!("{WITH_ELIF}f = lambda: 1\n"))?;
    let out = project.check(&[])?;
    assert_eq!(
        out.stdout,
        "src/a.py:3:1: t.elif no elif\nsrc/a.py:5:5: t.warn no elif\n\nFound 2 findings (1 error, 1 warning).\n"
    );
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn output_warnings_alone_exit_0_with_the_summary() -> Outcome {
    let project = Project::with_rules("[\"src\"]", &warn(ELIF))?;
    project.file("src/a.py", WITH_ELIF)?;
    let out = project.check(&[])?;
    assert_eq!(
        out.stdout,
        "src/a.py:3:1: t.warn no elif\n\nFound 1 finding (0 errors, 1 warning).\n"
    );
    assert_eq!(out.stderr, "");
    assert_eq!(out.code, 0);
    Ok(())
}

#[test]
fn output_text_is_the_default_format() -> Outcome {
    let project = Project::with_rules("[\"src\"]", ELIF)?;
    project.file("src/a.py", WITH_ELIF)?;
    let default = project.check(&[])?;
    let text = project.check(&["--output-format", "text"])?;
    assert_eq!(default.stdout, text.stdout);
    Ok(())
}

#[test]
fn output_json_is_pretty_with_stable_key_order() -> Outcome {
    let project = Project::with_rules("[\"src\"]", ELIF)?;
    project.file("src/a.py", WITH_ELIF)?;
    let out = project.check(&["--output-format", "json"])?;
    assert_eq!(out.code, 1);
    assert_eq!(out.stderr, "");
    assert!(
        out.stdout
            .starts_with("{\n  \"run\": {\n    \"tool\": \"0.1.0\",\n    \"date\": \""),
        "{}",
        out.stdout
    );
    assert!(out.stdout.ends_with("}\n"));
    let at = |key: &str| out.stdout.find(key).ok_or(format!("missing {key}"));
    let order = [
        "\"run\"",
        "\"findings\"",
        "\"summary\"",
        "\"by_severity\"",
        "\"by_tag\"",
        "\"by_rule\"",
    ];
    let positions = order.iter().map(|key| at(key)).collect::<Result<Vec<_>, _>>()?;
    assert!(positions.is_sorted(), "{positions:?}");
    let inner = [
        "\"rule\"",
        "\"path\"",
        "\"span\"",
        "\"message\"",
        "\"severity\"",
        "\"tags\"",
    ];
    let positions = inner.iter().map(|key| at(key)).collect::<Result<Vec<_>, _>>()?;
    assert!(positions.is_sorted(), "{positions:?}");
    Ok(())
}

#[test]
fn output_json_report_carries_run_findings_and_summary() -> Outcome {
    let project = Project::with_rules("[\"src\"]", ELIF)?;
    project.file("src/a.py", WITH_ELIF)?;
    let report = project.json(&[])?;
    assert_eq!(report["run"]["tool"], "0.1.0");
    assert_eq!(report["run"]["select"], json!(["ALL"]));
    assert_eq!(report["run"]["ignore"], json!([]));
    assert_eq!(report["run"]["paths"], json!(["src"]));
    let stamp = &report["run"]["catalogues"][0];
    assert_eq!((&stamp["name"], &stamp["version"]), (&json!("t"), &json!("1")));
    let digest = stamp["digest"].as_str().ok_or("no digest")?;
    assert_eq!(digest, GOLDEN_DIGEST);
    let date = report["run"]["date"].as_str().ok_or("no date")?;
    assert!(date.parse::<jiff::Timestamp>().is_ok(), "{date}");
    assert_eq!(
        report["findings"],
        json!([{
            "rule": "t.elif",
            "path": "src/a.py",
            "span": {
                "start": {"line": 3, "column": 1},
                "end": {"line": 4, "column": 9},
                "bytes": {"start": 15, "end": 31}
            },
            "message": "no elif",
            "severity": "error",
            "tags": {"law": "5"}
        }])
    );
    assert_eq!(
        report["summary"],
        json!({
            "total": 1,
            "by_severity": {"error": 1},
            "by_tag": {"law": {"5": 1}},
            "by_rule": {"t.elif": 1}
        })
    );
    Ok(())
}

#[test]
fn output_json_clean_run_is_an_empty_report() -> Outcome {
    let project = Project::with_rules("[\"src\"]", ELIF)?;
    project.file("src/a.py", "x = 1\n")?;
    let out = project.check(&["--output-format", "json"])?;
    assert_eq!(out.code, 0);
    let report: serde_json::Value = serde_json::from_str(&out.stdout)?;
    assert_eq!(report["findings"], json!([]));
    assert_eq!(report["summary"]["total"], 0);
    Ok(())
}

#[test]
fn output_json_findings_sorted_by_path_then_position() -> Outcome {
    let project = Project::with_rules("[\"src\"]", ELIF)?;
    project.file("src/b.py", WITH_ELIF)?;
    project.file("src/a.py", &format!("{WITH_ELIF}{WITH_ELIF}"))?;
    let report = project.json(&[])?;
    let at: Vec<(String, u64)> = report["findings"]
        .as_array()
        .ok_or("no findings")?
        .iter()
        .map(|f| {
            (
                f["path"].as_str().unwrap_or_default().to_owned(),
                f["span"]["start"]["line"].as_u64().unwrap_or_default(),
            )
        })
        .collect();
    assert_eq!(
        at,
        vec![
            ("src/a.py".to_owned(), 3),
            ("src/a.py".to_owned(), 7),
            ("src/b.py".to_owned(), 3)
        ]
    );
    Ok(())
}

#[test]
fn output_determinism_two_runs_are_identical_apart_from_the_date() -> Outcome {
    let project = Project::with_rules("[\"src\"]", ELIF)?;
    (0..40).try_for_each(|n| project.file(&format!("src/p{}/m{n}.py", n % 7), WITH_ELIF).map(drop))?;
    let first = project.check(&["--output-format", "json"])?;
    let second = project.check(&["--output-format", "json"])?;
    let third = project.check(&["--output-format", "json"])?;
    assert_eq!(first.code, 1);
    assert_eq!(without_date(&first.stdout), without_date(&second.stdout));
    assert_eq!(without_date(&first.stdout), without_date(&third.stdout));
    let text_a = project.check(&[])?;
    let text_b = project.check(&[])?;
    assert_eq!(text_a.stdout, text_b.stdout);
    Ok(())
}

#[test]
fn output_digest_is_sha256_of_64_hex_digits() -> Outcome {
    let project = Project::with_rules("[\"src\"]", ELIF)?;
    project.file("src/a.py", "x = 1\n")?;
    let digest = digest_of(&project)?;
    let hex = digest.strip_prefix("sha256:").ok_or("no sha256: prefix")?;
    assert_eq!(hex.len(), 64);
    assert!(hex.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')), "{digest}");
    Ok(())
}

#[test]
fn output_digest_is_the_same_across_runs_and_projects_and_ignores_the_code() -> Outcome {
    let first = Project::with_rules("[\"src\"]", ELIF)?;
    first.file("src/a.py", WITH_ELIF)?;
    let second = Project::with_rules("[\"src\"]", ELIF)?;
    second.file("src/other.py", "x = 1\n")?;
    assert_eq!(digest_of(&first)?, GOLDEN_DIGEST);
    assert_eq!(digest_of(&first)?, digest_of(&first)?);
    assert_eq!(digest_of(&first)?, digest_of(&second)?);
    Ok(())
}

#[test]
fn output_digest_changes_with_the_catalogue() -> Outcome {
    let first = Project::with_rules("[\"src\"]", ELIF)?;
    let second = Project::with_rules("[\"src\"]", &ELIF.replace("no elif", "no elif at all"))?;
    first.file("src/a.py", "x = 1\n")?;
    second.file("src/a.py", "x = 1\n")?;
    assert_ne!(digest_of(&first)?, digest_of(&second)?);
    Ok(())
}

#[test]
fn output_closed_pipe_exits_0_and_says_nothing() -> Outcome {
    let project = Project::with_rules("[\"src\"]", ELIF)?;
    (0..400).try_for_each(|n| project.file(&format!("src/p{n}.py"), WITH_ELIF).map(drop))?;
    let mut child = Command::new(env!("CARGO_BIN_EXE_law"))
        .current_dir(project.path())
        .arg("check")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    drop(child.stdout.take());
    let out = child.wait_with_output()?;
    assert_eq!(String::from_utf8(out.stderr)?, "");
    assert_eq!(out.status.code(), Some(0));
    Ok(())
}

#[test]
fn output_summary_grammar_singular_and_plural() -> Outcome {
    let one = Project::with_rules("[\"src\"]", ELIF)?;
    one.file("src/a.py", WITH_ELIF)?;
    assert!(
        one.check(&[])?
            .stdout
            .ends_with("Found 1 finding (1 error, 0 warnings).\n")
    );
    let two = Project::with_rules("[\"src\"]", ELIF)?;
    two.file("src/a.py", &format!("{WITH_ELIF}{WITH_ELIF}"))?;
    assert!(
        two.check(&[])?
            .stdout
            .ends_with("Found 2 findings (2 errors, 0 warnings).\n")
    );
    let warned = Project::with_rules("[\"src\"]", &warn(ELIF))?;
    warned.file("src/a.py", WITH_ELIF)?;
    assert!(
        warned
            .check(&[])?
            .stdout
            .ends_with("Found 1 finding (0 errors, 1 warning).\n")
    );
    Ok(())
}

#[test]
fn output_faults_end_the_text_and_never_claim_success() -> Outcome {
    let project = Project::with_rules("[\"src\"]", ELIF)?;
    project.bytes("src/bad.py", &[0xc3, 0x28])?;
    project.bytes("src/worse.py", &[0xff])?;
    let out = project.check(&[])?;
    assert_eq!(out.code, 2);
    assert_eq!(out.stdout, "2 faults: the run is incomplete.\n");
    Ok(())
}

#[test]
fn output_faults_follow_the_findings_line() -> Outcome {
    let project = Project::with_rules("[\"src\"]", ELIF)?;
    project.file("src/good.py", WITH_ELIF)?;
    project.bytes("src/bad.py", &[0xc3, 0x28])?;
    let out = project.check(&[])?;
    assert_eq!(
        out.stdout,
        "src/good.py:3:1: t.elif no elif\n\nFound 1 finding (1 error, 0 warnings).\n1 fault: the run is incomplete.\n"
    );
    Ok(())
}

#[test]
fn output_fault_paths_are_relative_to_the_project_root() -> Outcome {
    let project = Project::with_rules("[\"src\"]", ELIF)?;
    project.bytes("src/pkg/bad.py", &[0xc3, 0x28])?;
    let out = project.run_in("src/pkg", &["check"])?;
    assert!(out.stderr.starts_with("src/pkg/bad.py: "), "{}", out.stderr);
    assert!(
        !out.stderr.contains(project.path().to_string_lossy().as_ref()),
        "{}",
        out.stderr
    );
    Ok(())
}

#[test]
fn output_load_faults_print_no_report() -> Outcome {
    let project = Project::with_rules("[\"src\"]", "- id: not a rule\n")?;
    project.file("src/a.py", WITH_ELIF)?;
    let out = project.check(&[])?;
    assert_eq!(out.code, 2);
    assert_eq!(out.stdout, "");
    assert!(!out.stderr.is_empty());
    Ok(())
}

#[test]
fn output_unknown_format_exits_2_with_the_usage_error() -> Outcome {
    let project = Project::with_rules("[\"src\"]", ELIF)?;
    let out = project.check(&["--output-format", "yaml"])?;
    assert_eq!(out.code, 2);
    assert_eq!(out.stdout, "");
    assert!(out.stderr.contains("yaml"), "{}", out.stderr);
    Ok(())
}
