mod common;

use std::fs;
use std::process::Command;

use common::{Outcome, RUFF, TY, skip, tool_dir};
use law_engine::{Ignore, Noqa};
use serde_json::Value;

const CODE: &str = "F401";
const RULE: &str = "invalid-explicit-return";

fn noqa_silences(comment: &str) -> bool {
    Noqa::parse(comment).iter().any(|noqa| match noqa {
        Noqa::All => true,
        Noqa::Codes(codes) => codes.iter().any(|code| code == CODE),
    })
}

fn ignore_silences(comment: &str) -> bool {
    Ignore::parse(comment).iter().any(|ignore| match ignore {
        Ignore::All => true,
        Ignore::Names(names) => names.iter().any(|name| name == RULE),
    })
}

fn ruff_reports(source: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    fs::write(dir.path().join("a.py"), source)?;
    let out = Command::new(tool_dir(RUFF).ok_or("ruff")?.join(RUFF))
        .current_dir(dir.path())
        .args([
            "check",
            "--isolated",
            "--select",
            CODE,
            "--no-fix",
            "--output-format",
            "json",
            "--exit-zero",
            "a.py",
        ])
        .output()?;
    Ok(!serde_json::from_slice::<Vec<Value>>(&out.stdout)?.is_empty())
}

fn ty_reports(source: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    fs::write(dir.path().join("a.py"), source)?;
    let out = Command::new(tool_dir(TY).ok_or("ty")?.join(TY))
        .current_dir(dir.path())
        .args([
            "check",
            "--output-format",
            "sarif",
            "--exit-zero",
            "--no-progress",
            "--",
            "a.py",
        ])
        .output()?;
    let sarif: Value = serde_json::from_slice(&out.stdout)?;
    Ok(sarif["runs"][0]["results"]
        .as_array()
        .ok_or("results")?
        .iter()
        .any(|result| result["ruleId"] == RULE))
}

#[test]
fn grammar_noqa_parse_agrees_with_ruff_on_inline_comments() -> Outcome {
    if tool_dir(RUFF).is_none() {
        return skip(RUFF);
    }
    let comments = [
        "# noqa",
        "#noqa",
        "# NOQA",
        "# noqa:F401",
        "#noqa:F401",
        "# noqa: F401",
        "# NOQA : F401,W291",
        "# noqa: E501 F401",
        "# noqa: E501, F401",
        "# noqa:E501,F401",
        "# noqa : E501 F401",
        "# noqa:F401,",
        "# noqa: F401 # x",
        "# noqa F401",
        "# words # noqa: F401",
        "# noqa: F401, E501 trailing words",
        "# noqaF401",
        "# noqa:",
        "# noqa: ",
        "# noqa: E501",
        "# noqa: f401",
        "# noqa: F401abc",
        "# noqa: F",
        "# noqa: F4",
        "# noqa: F401F",
        "# nota noqa: F401",
        "# noqa: F401,E501",
        "# noqa:\tF401",
        "# noqa  :  F401",
    ];
    for comment in comments {
        let reports = ruff_reports(&format!("import os  {comment}\n"))?;
        assert_eq!(
            !reports,
            noqa_silences(comment),
            "ruff {} F401 under {comment:?}",
            if reports { "reports" } else { "silences" }
        );
    }
    Ok(())
}

#[test]
fn grammar_noqa_parse_agrees_with_ruff_on_file_level_comments() -> Outcome {
    if tool_dir(RUFF).is_none() {
        return skip(RUFF);
    }
    let comments = [
        "# ruff: noqa",
        "# ruff: noqa: F401",
        "# ruff:noqa",
        "# ruff : noqa",
        "# RUFF: NOQA",
        "# ruff: noqa: E501",
        "# ruff: noqa:F401",
        "# flake8: noqa",
        "# flake8: noqa: F401",
        "# flake8: noqa: E501",
        "# ruff: noqa F401",
        "# ruff: noqa:",
        "# ruff: ignore",
        "# ruffnoqa",
    ];
    for comment in comments {
        let reports = ruff_reports(&format!("{comment}\nimport os\n"))?;
        assert_eq!(
            !reports,
            noqa_silences(comment),
            "ruff {} F401 under {comment:?}",
            if reports { "reports" } else { "silences" }
        );
    }
    Ok(())
}

#[test]
fn grammar_ignore_parse_agrees_with_ty() -> Outcome {
    if tool_dir(TY).is_none() {
        return skip(TY);
    }
    let comments = [
        "# ty: ignore",
        "# ty:ignore",
        "#ty: ignore",
        "# type: ignore",
        "#type:ignore",
        "# ty: ignore[invalid-explicit-return]",
        "# ty: ignore[invalid-explicit-return, other]",
        "# ty: ignore[other, invalid-explicit-return]",
        "# ty: ignore[ invalid-explicit-return ]",
        "# ty: ignore [invalid-explicit-return]",
        "# ty: ignore[other]",
        "# ty: ignore[]",
        "# type: ignore[]",
        "# type: ignore[ty:invalid-explicit-return]",
        "# type: ignore[invalid-explicit-return]",
        "# type: ignore[attr-defined]",
        "# ty: ignore[invalid-explicit-return",
        "# ty: ignored",
        "# ty: ignore  # because",
        "# type: ignore[ty:other]",
    ];
    for comment in comments {
        let reports = ty_reports(&format!("def f() -> str:\n    return 1  {comment}\n"))?;
        assert_eq!(
            !reports,
            ignore_silences(comment),
            "ty {} the error under {comment:?}",
            if reports { "reports" } else { "silences" }
        );
    }
    Ok(())
}
