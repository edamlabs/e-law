mod common;

use common::{Outcome, Project, found, passed};

fn rule(check: &str) -> String {
    format!(
        "- id: t.logic\n  message: logic holds\n  check: {{engine: tree, {check}}}\n  fix: {{safety: none, reason: x}}\n"
    )
}

fn project(check: &str) -> Result<Project, Box<dyn std::error::Error>> {
    Project::with_rules("[\"src\"]", &rule(check))
}

const ALLOW: &str = "dirs: [\"src/*/logic\"], allow: [\"README.md\", \"*/\"]";

#[test]
fn tree_stranger_is_a_finding_at_line_1_of_the_directory() -> Outcome {
    let project = project(ALLOW)?;
    project.file("src/pkg/logic/README.md", "")?;
    project.file("src/pkg/logic/x.py", "")?;
    let out = project.check(&[])?;
    assert_eq!(
        out.stdout,
        found("src/pkg/logic:1:1", "t.logic", "logic holds: x.py is not allowed", 1, 0)
    );
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn tree_allowed_entries_pass() -> Outcome {
    let project = project(ALLOW)?;
    project.file("src/pkg/logic/README.md", "")?;
    project.file("src/pkg/logic/minutes/a.py", "")?;
    project.file("src/pkg/logic/other/b.py", "")?;
    assert!(passed(&project.check(&[])?));
    Ok(())
}

#[test]
fn tree_subdirectory_is_written_with_a_trailing_slash() -> Outcome {
    let project = project("dirs: [\"src/*/logic\"], allow: [\"README.md\"]")?;
    project.file("src/pkg/logic/README.md", "")?;
    project.file("src/pkg/logic/minutes/a.py", "")?;
    let out = project.check(&[])?;
    assert_eq!(
        out.stdout,
        found(
            "src/pkg/logic:1:1",
            "t.logic",
            "logic holds: minutes/ is not allowed",
            1,
            0
        )
    );
    Ok(())
}

#[test]
fn tree_missing_required_is_a_finding_naming_the_glob() -> Outcome {
    let project = project("dirs: [\"src/*/logic\"], require: [\"README.md\"]")?;
    project.file("src/pkg/logic/a.py", "")?;
    let out = project.check(&[])?;
    assert_eq!(
        out.stdout,
        found(
            "src/pkg/logic:1:1",
            "t.logic",
            "logic holds: README.md is missing",
            1,
            0
        )
    );
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn tree_required_glob_met_by_any_entry_passes() -> Outcome {
    let project = project("dirs: [\"src/*/logic\"], require: [\"*.md\"]")?;
    project.file("src/pkg/logic/NOTES.md", "")?;
    assert!(passed(&project.check(&[])?));
    Ok(())
}

#[test]
fn tree_pycache_is_ignored_as_entry_and_as_directory() -> Outcome {
    let project = project("dirs: [\"src/pkg/logic/*\"], require: [\"x.py\"], allow: [\"x.py\"]")?;
    project.file("src/pkg/logic/sub/x.py", "")?;
    project.file("src/pkg/logic/sub/__pycache__/x.cpython-314.pyc", "")?;
    project.file("src/pkg/logic/__pycache__/y.pyc", "")?;
    assert!(passed(&project.check(&[])?));
    Ok(())
}

#[test]
fn tree_name_expression_judges_every_entry() -> Outcome {
    let project = project("dirs: [\"src/*/logic\"], name: \"[a-z]+(\\\\.py)?\"")?;
    project.file("src/pkg/logic/good.py", "")?;
    project.file("src/pkg/logic/sub/a.py", "")?;
    project.file("src/pkg/logic/Bad.py", "")?;
    let out = project.check(&[])?;
    assert_eq!(
        out.stdout,
        found(
            "src/pkg/logic:1:1",
            "t.logic",
            "logic holds: Bad.py does not match [a-z]+(\\.py)?",
            1,
            0
        )
    );
    Ok(())
}

#[test]
fn tree_only_fences_the_entries_the_name_expression_judges() -> Outcome {
    let project = project("dirs: [\"src/*/logic\"], only: [\"*.py\", \"*/\"], name: \"[a-z]+(\\\\.py)?\"")?;
    project.file("src/pkg/logic/Bad.py", "")?;
    project.file("src/pkg/logic/Bad.json", "")?;
    project.file("src/pkg/logic/Sub/a.py", "")?;
    let out = project.check(&[])?;
    assert_eq!(
        out.stdout,
        "src/pkg/logic:1:1: t.logic logic holds: Bad.py does not match [a-z]+(\\.py)?\nsrc/pkg/logic:1:1: t.logic logic holds: Sub/ does not match [a-z]+(\\.py)?\n\nFound 2 findings (2 errors, 0 warnings).\n"
    );
    Ok(())
}

#[test]
fn tree_entry_breaking_allow_and_name_is_one_finding_per_reason() -> Outcome {
    let project = project("dirs: [\"src/*/logic\"], allow: [\"README.md\"], name: \"[a-z]+(\\\\.py)?\"")?;
    project.file("src/pkg/logic/README.md", "")?;
    project.file("src/pkg/logic/Bad.py", "")?;
    let out = project.check(&[])?;
    assert_eq!(
        out.stdout,
        "src/pkg/logic:1:1: t.logic logic holds: Bad.py does not match [a-z]+(\\.py)?\nsrc/pkg/logic:1:1: t.logic logic holds: Bad.py is not allowed\nsrc/pkg/logic:1:1: t.logic logic holds: README.md does not match [a-z]+(\\.py)?\n\nFound 3 findings (3 errors, 0 warnings).\n"
    );
    Ok(())
}

#[test]
fn tree_scope_include_does_not_restrict_dirs_but_exclude_fences() -> Outcome {
    let fenced = format!("{}  scope: {{exclude: [\"src/pkg/**\"]}}\n", rule(ALLOW));
    let project = Project::with_rules("[\"src\"]", &fenced)?;
    project.file("src/pkg/logic/x.py", "")?;
    assert!(passed(&project.check(&[])?));

    let open = Project::with_rules(
        "[\"src\"]",
        &format!("{}  scope: {{include: [\"nowhere/**\"]}}\n", rule(ALLOW)),
    )?;
    open.file("src/pkg/logic/x.py", "")?;
    assert_eq!(
        open.check(&[])?.stdout,
        found("src/pkg/logic:1:1", "t.logic", "logic holds: x.py is not allowed", 1, 0)
    );
    Ok(())
}

#[test]
fn tree_every_matching_directory_is_judged_in_path_order() -> Outcome {
    let project = project(ALLOW)?;
    project.file("src/zeta/logic/z.py", "")?;
    project.file("src/alpha/logic/a.py", "")?;
    let out = project.check(&[])?;
    assert_eq!(
        out.stdout,
        "src/alpha/logic:1:1: t.logic logic holds: a.py is not allowed\nsrc/zeta/logic:1:1: t.logic logic holds: z.py is not allowed\n\nFound 2 findings (2 errors, 0 warnings).\n"
    );
    Ok(())
}

#[test]
fn tree_deny_flags_each_entry_a_glob_names_and_walks_every_depth() -> Outcome {
    let project = project("dirs: [\"src\", \"src/**\"], deny: [\"core/\", \"utils.py\"]")?;
    project.file("src/pkg/engines/core/a.py", "")?;
    project.file("src/pkg/engines/utils.py", "")?;
    project.file("src/pkg/engines/cache.py", "")?;
    project.file("src/pkg/corelib/b.py", "")?;
    let out = project.check(&[])?;
    assert_eq!(
        out.stdout,
        "src/pkg/engines:1:1: t.logic logic holds: core/ is banned\nsrc/pkg/engines:1:1: t.logic logic holds: utils.py is banned\n\nFound 2 findings (2 errors, 0 warnings).\n"
    );
    Ok(())
}

#[test]
fn tree_deny_leaves_a_clean_tree_alone() -> Outcome {
    let project = project("dirs: [\"src\", \"src/**\"], deny: [\"core/\"]")?;
    project.file("src/pkg/engines/cache.py", "")?;
    assert!(passed(&project.check(&[])?));
    Ok(())
}
