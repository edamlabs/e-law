mod common;

use common::{ELIF, Outcome, Project, WITH_ELIF, passed};

const FILE: &str = "law.exceptions.toml";

fn exception(rule: &str, target: &str, expires: Option<&str>) -> String {
    let expiry = expires.map_or_else(String::new, |day| format!("expires = \"{day}\"\n"));
    format!("[[exception]]\nrule = \"{rule}\"\n{target}\nreason = \"legacy\"\napproved = \"damien\"\n{expiry}\n")
}

fn project(exceptions: &str) -> Result<Project, Box<dyn std::error::Error>> {
    let project = Project::with_rules("[\"src\"]", ELIF)?;
    project.config(&format!(
        "roots = [\"src\"]\ncatalogues = [\"cat/\"]\nexceptions = \"{FILE}\"\n"
    ))?;
    project.file(FILE, exceptions)?;
    project.file("src/pkg/a.py", WITH_ELIF)?;
    project.file("src/pkg/b.py", WITH_ELIF)?;
    Ok(project)
}

#[test]
fn exception_silences_by_path() -> Outcome {
    let project = project(&exception("t.elif", "path = \"src/pkg/a.py\"", None))?;
    let out = project.check(&[])?;
    assert_eq!(
        out.stdout,
        "src/pkg/b.py:3:1: t.elif no elif\n\nFound 1 finding (1 error, 0 warnings).\n"
    );
    assert_eq!(out.stderr, "");
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn exception_path_glob_and_every_selector_form_silence() -> Outcome {
    for selector in ["t.elif", "t.*", "law=5", "ALL"] {
        let project = project(&exception(selector, "path = \"src/pkg/*.py\"", None))?;
        assert!(passed(&project.check(&[])?), "selector {selector}");
    }
    Ok(())
}

#[test]
fn exception_of_another_rule_silences_nothing_and_is_unused() -> Outcome {
    let project = project(&exception("t.other", "path = \"src/pkg/*.py\"", None))?;
    let out = project.check(&[])?;
    assert_eq!(
        out.stdout,
        "law.exceptions.toml:1:1: law.exception-unused the exception silences nothing: remove it: `t.other` at src/pkg/*.py\nsrc/pkg/a.py:3:1: t.elif no elif\nsrc/pkg/b.py:3:1: t.elif no elif\n\nFound 3 findings (3 errors, 0 warnings).\n"
    );
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn exception_expired_does_not_silence_and_is_reported() -> Outcome {
    let project = project(&exception("t.elif", "path = \"src/pkg/a.py\"", Some("2000-01-01")))?;
    let out = project.check(&[])?;
    assert_eq!(
        out.stdout,
        "law.exceptions.toml:1:1: law.exception-expired the exception has expired: renew it or remove it: `t.elif` at src/pkg/a.py\nsrc/pkg/a.py:3:1: t.elif no elif\nsrc/pkg/b.py:3:1: t.elif no elif\n\nFound 3 findings (3 errors, 0 warnings).\n"
    );
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn exception_expiring_in_the_future_silences() -> Outcome {
    let project = project(&exception("t.elif", "path = \"src/pkg/*.py\"", Some("2999-12-31")))?;
    assert!(passed(&project.check(&[])?));
    Ok(())
}

#[test]
fn exception_unused_is_reported_in_a_whole_tree_run() -> Outcome {
    let project = project(&format!(
        "{}{}",
        exception("t.elif", "path = \"src/pkg/*.py\"", None),
        exception("t.elif", "path = \"src/gone/*.py\"", None)
    ))?;
    let out = project.check(&[])?;
    assert_eq!(
        out.stdout,
        "law.exceptions.toml:1:1: law.exception-unused the exception silences nothing: remove it: `t.elif` at src/gone/*.py\n\nFound 1 finding (1 error, 0 warnings).\n"
    );
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn exception_unused_is_not_reported_when_paths_are_given() -> Outcome {
    let project = project(&exception("t.elif", "path = \"src/gone/*.py\"", None))?;
    let out = project.check(&["src/pkg/a.py"])?;
    assert_eq!(
        out.stdout,
        "src/pkg/a.py:3:1: t.elif no elif\n\nFound 1 finding (1 error, 0 warnings).\n"
    );
    Ok(())
}

#[test]
fn exception_reserved_rules_cannot_be_deselected() -> Outcome {
    let project = project(&exception("t.other", "path = \"src/*\"", Some("2000-01-01")))?;
    let out = project.check(&[
        "--ignore",
        "law.exception-expired",
        "--ignore",
        "law.exception-unused",
        "--select",
        "t.elif",
    ])?;
    assert_eq!(
        out.stdout,
        "law.exceptions.toml:1:1: law.exception-expired the exception has expired: renew it or remove it: `t.other` at src/*\nsrc/pkg/a.py:3:1: t.elif no elif\nsrc/pkg/b.py:3:1: t.elif no elif\n\nFound 3 findings (3 errors, 0 warnings).\n"
    );
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn exception_reserved_rules_cannot_be_silenced_by_an_exception() -> Outcome {
    let both = format!(
        "{}{}",
        exception("law.exception-expired", "path = \"law.exceptions.toml\"", None),
        exception("t.other", "path = \"src/*\"", Some("2000-01-01"))
    );
    let out = project(&both)?.check(&[])?;
    assert!(
        out.stdout.contains("law.exception-expired the exception has expired"),
        "{}",
        out.stdout
    );
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn exception_with_symbol_is_a_fault() -> Outcome {
    let project = project(&exception("t.elif", "symbol = \"pkg.a.run\"", None))?;
    let out = project.check(&[])?;
    assert_eq!(out.code, 2);
    assert_eq!(out.stdout, "");
    assert_eq!(out.stderr, "exception to `t.elif`: `symbol` is not implemented\n");
    Ok(())
}

#[test]
fn exception_with_path_and_symbol_is_a_fault_too() -> Outcome {
    let project = project(&exception("t.elif", "path = \"src/pkg/a.py\"\nsymbol = \"run\"", None))?;
    let out = project.check(&[])?;
    assert_eq!(out.code, 2);
    assert_eq!(out.stderr, "exception to `t.elif`: `symbol` is not implemented\n");
    Ok(())
}

#[test]
fn exception_without_path_or_symbol_is_a_fault() -> Outcome {
    let project = project(&exception("t.elif", "", None))?;
    let out = project.check(&[])?;
    assert_eq!(out.code, 2);
    assert_eq!(out.stdout, "");
    assert!(out.stderr.contains(FILE), "{}", out.stderr);
    Ok(())
}
