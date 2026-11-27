mod common;

use std::fs;
use std::process::Command;

use common::{ELIF, MIXED_FILE, Out, Outcome, Project, WITH_ELIF, git, mixed};
use serde_json::{Value, json};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/report");
const FORMATS: [&str; 4] = ["text", "json", "sarif", "md"];
const DATE: &str = "2026-01-02T03:04:05Z";
const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

type Fault = Box<dyn std::error::Error>;

fn golden(name: &str) -> Result<String, Fault> {
    Ok(fs::read_to_string(format!("{FIXTURES}/{name}"))?)
}

fn converted(project: &Project, path: &str, format: &str) -> Result<Out, Fault> {
    project.run(&["report", path, "--format", format])
}

fn saved(project: &Project) -> Result<String, Fault> {
    let out = project.check(&["--output-format", "json"])?;
    project.file("run.json", &out.stdout)?;
    Ok(out.stdout)
}

fn without_date(text: &str) -> String {
    text.lines()
        .map(|line| match line.split_once(" \u{b7} ") {
            Some((head, rest)) if head.starts_with("law ") => {
                let parts: Vec<&str> = rest.split(" \u{b7} ").filter(|part| !part.contains('T')).collect();
                format!("{head} \u{b7} {}", parts.join(" \u{b7} "))
            }
            _ => line.to_owned(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn report_each_format_of_a_saved_run_is_its_golden_file() -> Outcome {
    let project = Project::new()?;
    for name in ["mixed", "clean"] {
        for (format, extension) in [("text", "text"), ("md", "md"), ("sarif", "sarif")] {
            let out = converted(&project, &format!("{FIXTURES}/{name}.json"), format)?;
            assert_eq!((out.code, out.stderr.as_str()), (0, ""), "{name} {format}");
            assert_eq!(out.stdout, golden(&format!("{name}.{extension}"))?, "{name} {format}");
        }
        let json = converted(&project, &format!("{FIXTURES}/{name}.json"), "json")?;
        assert_eq!(json.stdout, golden(&format!("{name}.json"))?, "{name} json");
    }
    Ok(())
}

#[test]
fn report_md_holds_header_summary_by_tag_key_and_findings_by_rule() -> Outcome {
    let md = golden("mixed.md")?;
    let lines: Vec<&str> = md.lines().collect();
    assert_eq!(
        lines[0],
        format!("law 0.1.0 \u{b7} commit {COMMIT} \u{b7} {DATE} \u{b7} catalogues: t 1")
    );
    assert_eq!(lines[2], "28 findings, 4 rules hit.");
    let tables: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| line.starts_with("| ") && line.contains("| findings |"))
        .collect();
    assert_eq!(
        tables,
        [
            "| law | findings | rules hit |",
            "| kit | findings | rules hit |",
            "| set | findings | rules hit |"
        ]
    );
    let laws: Vec<&str> = lines
        .iter()
        .copied()
        .skip_while(|line| *line != "| law | findings | rules hit |")
        .skip(2)
        .take_while(|line| line.starts_with("| "))
        .collect();
    assert_eq!(
        laws,
        [
            "| 2 | 25 | 1 |",
            "| 5 | 1 | 1 |",
            "| 10 | 1 | 1 |",
            "| (untagged) | 1 | 1 |"
        ]
    );
    let rules: Vec<&str> = lines.iter().copied().filter(|line| line.starts_with("### ")).collect();
    assert_eq!(
        rules,
        [
            "### `t.elif` (1)",
            "### `t.lambda` (25)",
            "### `t.swap` (1)",
            "### `t.untagged` (1)"
        ]
    );
    assert!(!md.contains('\u{1b}') && !md.contains('<'), "no colour, no HTML");
    Ok(())
}

#[test]
fn report_md_caps_a_rule_at_twenty_lines_and_says_how_many_more() -> Outcome {
    let md = golden("mixed.md")?;
    let lambda: Vec<&str> = md
        .lines()
        .skip_while(|line| *line != "### `t.lambda` (25)")
        .skip(1)
        .take_while(|line| !line.starts_with("### "))
        .filter(|line| line.starts_with("- "))
        .collect();
    assert_eq!(lambda.len(), 21);
    assert_eq!(lambda[0], "- `pk/other/many.py:1`");
    assert_eq!(lambda[19], "- `pk/other/many.py:20`");
    assert_eq!(lambda[20], "- \u{2026} and 5 more");
    Ok(())
}

#[test]
fn report_md_of_exactly_twenty_findings_has_no_more_line() -> Outcome {
    let project = Project::with_rules("[\"src\"]", ELIF)?;
    project.file("src/a.py", &WITH_ELIF.repeat(20))?;
    let out = project.check(&["--output-format", "md"])?;
    assert!(
        out.stdout.contains("- `src/a.py:3`") && out.stdout.contains("(20)"),
        "{}",
        out.stdout
    );
    assert!(!out.stdout.contains("more"), "{}", out.stdout);
    Ok(())
}

#[test]
fn report_md_of_a_clean_run_is_the_header_alone() -> Outcome {
    assert_eq!(
        golden("clean.md")?,
        format!("law 0.1.0 \u{b7} {DATE} \u{b7} catalogues: t 1\n\n0 findings, 0 rules hit.\n")
    );
    Ok(())
}

fn without_date_json(text: &str) -> Result<Value, Fault> {
    let mut report: Value = serde_json::from_str(text)?;
    report["run"]["date"] = json!("-");
    Ok(report)
}

#[test]
fn report_converting_a_saved_run_equals_the_direct_output() -> Outcome {
    let project = mixed()?;
    saved(&project)?;
    for format in ["text", "sarif"] {
        let direct = project.check(&["--output-format", format])?;
        let after = converted(&project, "run.json", format)?;
        assert_eq!((after.code, after.stderr.as_str()), (0, ""), "{format}");
        assert_eq!(after.stdout, direct.stdout, "{format}");
    }
    let direct = project.check(&["--output-format", "json"])?;
    let after = converted(&project, "run.json", "json")?;
    assert_eq!(without_date_json(&after.stdout)?, without_date_json(&direct.stdout)?);
    Ok(())
}

#[test]
fn report_json_of_a_saved_run_is_the_saved_file_byte_for_byte() -> Outcome {
    let project = mixed()?;
    let kept = saved(&project)?;
    assert_eq!(converted(&project, "run.json", "json")?.stdout, kept);
    Ok(())
}

#[test]
fn report_md_of_a_saved_run_is_the_direct_md_but_for_the_moment_it_ran() -> Outcome {
    let project = mixed()?;
    saved(&project)?;
    let direct = project.check(&["--output-format", "md"])?;
    let after = converted(&project, "run.json", "md")?;
    assert_eq!(without_date(&after.stdout), without_date(&direct.stdout));
    assert_eq!(converted(&project, "run.json", "md")?.stdout, after.stdout);
    Ok(())
}

#[test]
fn report_default_format_is_text_and_exit_is_zero_with_findings() -> Outcome {
    let project = mixed()?;
    saved(&project)?;
    let default = project.run(&["report", "run.json"])?;
    assert_eq!(default.stdout, project.check(&[])?.stdout);
    assert_eq!(default.code, 0);
    Ok(())
}

#[test]
fn report_needs_no_project() -> Outcome {
    let project = Project::new()?;
    for format in FORMATS {
        let out = converted(&project, &format!("{FIXTURES}/mixed.json"), format)?;
        assert_eq!((out.code, out.stderr.as_str()), (0, ""), "{format}");
        assert!(!out.stdout.is_empty(), "{format}");
    }
    Ok(())
}

#[test]
fn report_a_run_saved_before_rules_were_recorded_still_converts() -> Outcome {
    let project = Project::new()?;
    let mut report: Value = serde_json::from_str(&golden("mixed.json")?)?;
    report.as_object_mut().ok_or("object")?.remove("rules");
    project.file("old.json", &report.to_string())?;
    let sarif: Value = serde_json::from_str(&converted(&project, "old.json", "sarif")?.stdout)?;
    let rules = sarif["runs"][0]["tool"]["driver"]["rules"].as_array().ok_or("rules")?;
    assert!(rules.iter().all(|rule| rule["shortDescription"]["text"] == rule["id"]));
    let md = converted(&project, "old.json", "md")?;
    assert_eq!(md.code, 0);
    assert!(md.stdout.contains("### `t.swap` (1)"));
    Ok(())
}

#[test]
fn report_a_bad_file_is_exit_2_with_the_path_and_nothing_on_stdout() -> Outcome {
    let project = Project::new()?;
    let valid: Value = serde_json::from_str(&golden("mixed.json")?)?;
    let mut extra = valid.clone();
    extra["surprise"] = json!(1);
    let mut dated = valid.clone();
    dated["run"]["date"] = json!("yesterday");
    let mut missing = valid.clone();
    missing.as_object_mut().ok_or("object")?.remove("summary");
    for (name, text) in [
        ("empty.json", String::new()),
        ("prose.json", "not json".to_owned()),
        ("array.json", "[]".to_owned()),
        ("extra.json", extra.to_string()),
        ("dated.json", dated.to_string()),
        ("missing.json", missing.to_string()),
    ] {
        project.file(name, &text)?;
        let out = converted(&project, name, "sarif")?;
        assert_eq!((out.code, out.stdout.as_str()), (2, ""), "{name}");
        assert!(
            out.stderr.starts_with(&format!("{name}: not a report: ")),
            "{name}: {}",
            out.stderr
        );
    }
    let absent = converted(&project, "absent.json", "text")?;
    assert_eq!((absent.code, absent.stdout.as_str()), (2, ""));
    assert!(absent.stderr.starts_with("absent.json: "), "{}", absent.stderr);
    Ok(())
}

#[test]
fn report_an_unknown_format_is_refused_before_reading() -> Outcome {
    let project = Project::new()?;
    let out = converted(&project, "absent.json", "html")?;
    assert_eq!(out.code, 2);
    assert!(out.stderr.contains("html"), "{}", out.stderr);
    Ok(())
}

#[test]
fn report_commit_is_the_head_of_the_repository_the_run_started_in() -> Outcome {
    let project = mixed()?;
    git(project.path(), &["init", "-q"])?;
    git(project.path(), &["add", "."])?;
    git(project.path(), &["commit", "-q", "-m", "x"])?;
    let head = git(project.path(), &["rev-parse", "HEAD"])?;
    let report = project.json(&[])?;
    assert_eq!(report["run"]["commit"], head);
    let md = project.check(&["--output-format", "md"])?;
    assert!(
        md.stdout
            .lines()
            .next()
            .is_some_and(|line| line.contains(&format!("commit {head}"))),
        "{}",
        md.stdout
    );
    saved(&project)?;
    let after = converted(&project, "run.json", "md")?;
    assert!(after.stdout.contains(&format!("commit {head}")));
    Ok(())
}

#[test]
fn report_commit_is_absent_outside_a_repository_and_in_one_without_a_commit() -> Outcome {
    let outside = mixed()?;
    let report = outside.json(&[])?;
    assert!(report["run"].get("commit").is_none(), "{}", report["run"]);
    let md = outside.check(&["--output-format", "md"])?;
    assert!(!md.stdout.contains("commit"), "{}", md.stdout);
    let unborn = mixed()?;
    git(unborn.path(), &["init", "-q"])?;
    let out = unborn.check(&["--output-format", "json"])?;
    assert_eq!((out.code, out.stderr.as_str()), (1, ""));
    assert!(
        serde_json::from_str::<Value>(&out.stdout)?["run"]
            .get("commit")
            .is_none()
    );
    Ok(())
}

#[test]
fn report_commit_is_that_of_the_project_root_not_of_the_working_directory() -> Outcome {
    let project = mixed()?;
    git(project.path(), &["init", "-q"])?;
    git(project.path(), &["add", "."])?;
    git(project.path(), &["commit", "-q", "-m", "x"])?;
    let head = git(project.path(), &["rev-parse", "HEAD"])?;
    let out = project.run_in("pk", &["check", "--output-format", "json"])?;
    assert_eq!(serde_json::from_str::<Value>(&out.stdout)?["run"]["commit"], head);
    Ok(())
}

#[test]
fn report_git_absent_from_the_path_is_no_fault() -> Outcome {
    let project = mixed()?;
    git(project.path(), &["init", "-q"])?;
    let empty = tempfile::tempdir()?;
    let out = Command::new(env!("CARGO_BIN_EXE_law"))
        .current_dir(project.path())
        .env("PATH", empty.path())
        .args(["check", "--output-format", "json"])
        .output()?;
    assert_eq!(out.stderr, b"");
    assert!(
        serde_json::from_slice::<Value>(&out.stdout)?["run"]
            .get("commit")
            .is_none()
    );
    Ok(())
}

#[test]
fn report_rules_record_the_message_of_every_rule_that_fired_and_only_those() -> Outcome {
    let report = mixed()?.json(&[])?;
    assert_eq!(
        report["rules"],
        json!({"t.elif": "no elif", "t.lambda": "no lambda", "t.swap": "use new", "t.untagged": "untagged rule"})
    );
    let quiet = Project::with_rules("[\"src\"]", ELIF)?;
    quiet.file("src/a.py", "x = 1\n")?;
    assert!(quiet.json(&[])?.get("rules").is_none());
    Ok(())
}

#[test]
fn report_paths_with_a_space_and_a_non_ascii_character_survive_every_format() -> Outcome {
    let project = mixed()?;
    saved(&project)?;
    for format in ["text", "md", "json"] {
        let out = converted(&project, "run.json", format)?;
        assert!(out.stdout.contains(MIXED_FILE), "{format}");
    }
    Ok(())
}
