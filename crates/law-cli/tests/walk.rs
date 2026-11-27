mod common;

use common::{ELIF, Outcome, Project, WITH_ELIF, found, passed};

fn project() -> Result<Project, Box<dyn std::error::Error>> {
    Project::with_rules("[\"src\"]", ELIF)
}

fn hit(path: &str) -> String {
    format!("{path}:3:1: t.elif no elif\n")
}

#[test]
fn walk_gitignored_files_and_directories_are_not_checked() -> Outcome {
    let project = project()?;
    project.file(".gitignore", "gen/\nskip.py\n")?;
    project.file("src/pkg/a.py", WITH_ELIF)?;
    project.file("src/pkg/skip.py", WITH_ELIF)?;
    project.file("src/pkg/gen/b.py", WITH_ELIF)?;
    let out = project.check(&[])?;
    assert_eq!(out.stdout, found("src/pkg/a.py:3:1", "t.elif", "no elif", 1, 0));
    Ok(())
}

#[test]
fn walk_nested_gitignore_applies_below_its_directory_only() -> Outcome {
    let project = project()?;
    project.file("src/pkg/.gitignore", "local.py\n")?;
    project.file("src/pkg/local.py", WITH_ELIF)?;
    project.file("src/other/local.py", WITH_ELIF)?;
    let out = project.check(&[])?;
    assert_eq!(out.stdout, found("src/other/local.py:3:1", "t.elif", "no elif", 1, 0));
    Ok(())
}

#[test]
fn walk_explicit_directory_restricts_the_run() -> Outcome {
    let project = project()?;
    project.file("src/pkg/a.py", WITH_ELIF)?;
    project.file("src/other/b.py", WITH_ELIF)?;
    let out = project.check(&["src/pkg"])?;
    assert_eq!(out.stdout, found("src/pkg/a.py:3:1", "t.elif", "no elif", 1, 0));
    Ok(())
}

#[test]
fn walk_explicit_file_restricts_the_run_and_several_paths_union() -> Outcome {
    let project = project()?;
    project.file("src/pkg/a.py", WITH_ELIF)?;
    project.file("src/pkg/c.py", WITH_ELIF)?;
    project.file("src/other/b.py", WITH_ELIF)?;
    let one = project.check(&["src/pkg/a.py"])?;
    assert_eq!(one.stdout, found("src/pkg/a.py:3:1", "t.elif", "no elif", 1, 0));
    let two = project.check(&["src/pkg/a.py", "src/other"])?;
    assert_eq!(
        two.stdout,
        format!(
            "{}{}\nFound 2 findings (2 errors, 0 warnings).\n",
            hit("src/other/b.py"),
            hit("src/pkg/a.py")
        )
    );
    Ok(())
}

#[test]
fn walk_explicit_path_outside_the_roots_is_still_checked() -> Outcome {
    let project = project()?;
    project.file("scripts/s.py", WITH_ELIF)?;
    project.file("src/pkg/a.py", "")?;
    let rules = ELIF.replace("src/**/*.py", "**/*.py");
    project.rules(&rules)?;
    let out = project.check(&["scripts"])?;
    assert_eq!(out.stdout, found("scripts/s.py:3:1", "t.elif", "no elif", 1, 0));
    Ok(())
}

#[test]
fn walk_hidden_entries_and_non_python_files_are_skipped() -> Outcome {
    let project = project()?;
    project.file("src/.hidden.py", WITH_ELIF)?;
    project.file("src/.dir/x.py", WITH_ELIF)?;
    project.file("src/notes.txt", WITH_ELIF)?;
    project.file("src/pkg/__pycache__/a.py", WITH_ELIF)?;
    assert!(passed(&project.check(&[])?));
    Ok(())
}

#[test]
fn walk_non_utf8_file_is_a_fault_not_a_panic() -> Outcome {
    let project = project()?;
    project.file("src/good.py", WITH_ELIF)?;
    project.bytes("src/bad.py", &[0x78, 0x20, 0x3d, 0x20, 0xff, 0xfe, 0xfd, 0x0a])?;
    let out = project.check(&[])?;
    assert_eq!(out.code, 2);
    assert!(!out.stderr.contains("panicked"), "{}", out.stderr);
    assert_eq!(out.stderr.lines().count(), 1, "{}", out.stderr);
    assert!(out.stderr.starts_with("src/bad.py: "), "{}", out.stderr);
    assert_eq!(
        out.stdout,
        format!(
            "{}1 fault: the run is incomplete.\n",
            found("src/good.py:3:1", "t.elif", "no elif", 1, 0)
        )
    );
    Ok(())
}

#[test]
fn walk_non_utf8_file_alone_reports_the_fault_and_never_claims_success() -> Outcome {
    let project = project()?;
    project.bytes("src/bad.py", &[0xc3, 0x28])?;
    let out = project.check(&[])?;
    assert_eq!(out.code, 2);
    assert_eq!(out.stdout, "1 fault: the run is incomplete.\n");
    assert!(out.stderr.starts_with("src/bad.py: "), "{}", out.stderr);
    Ok(())
}

#[test]
fn walk_syntax_error_in_a_file_does_not_panic() -> Outcome {
    let project = project()?;
    project.file("src/broken.py", "def (:\n    elif\n")?;
    let out = project.check(&[])?;
    assert!(!out.stderr.contains("panicked"), "{}", out.stderr);
    assert!(matches!(out.code, 0..=2), "{out:?}");
    Ok(())
}

#[test]
fn walk_existing_path_outside_the_project_is_an_outside_fault() -> Outcome {
    let project = project()?;
    project.file("src/a.py", "")?;
    let elsewhere = Project::new()?;
    elsewhere.file("x.py", WITH_ELIF)?;
    let target = elsewhere.path().to_string_lossy().into_owned();
    let out = project.check(&[&target])?;
    assert_eq!(out.code, 2);
    assert_eq!(out.stderr, format!("{target}: outside the project root\n"));
    Ok(())
}

#[test]
fn walk_missing_path_is_a_fault_naming_it() -> Outcome {
    let project = project()?;
    project.file("src/a.py", "")?;
    let out = project.check(&["src/missing.py"])?;
    assert_eq!(out.code, 2);
    assert_eq!(out.stderr.lines().count(), 1, "{}", out.stderr);
    assert!(out.stderr.contains("src/missing.py: No such file"), "{}", out.stderr);
    Ok(())
}

#[test]
fn walk_discovers_the_project_from_a_subdirectory() -> Outcome {
    let project = project()?;
    project.file("src/pkg/a.py", WITH_ELIF)?;
    project.file("src/other/b.py", WITH_ELIF)?;
    let whole = project.run_in("src/pkg", &["check"])?;
    assert_eq!(
        whole.stdout,
        format!(
            "{}{}\nFound 2 findings (2 errors, 0 warnings).\n",
            hit("src/other/b.py"),
            hit("src/pkg/a.py")
        )
    );
    let here = project.run_in("src/pkg", &["check", "a.py"])?;
    assert_eq!(here.stdout, found("src/pkg/a.py:3:1", "t.elif", "no elif", 1, 0));
    Ok(())
}

#[test]
fn walk_config_flag_names_the_configuration() -> Outcome {
    let project = project()?;
    project.file("src/pkg/a.py", WITH_ELIF)?;
    project.file("other.toml", "roots = [\"elsewhere\"]\ncatalogues = [\"cat/\"]\n")?;
    project.file("elsewhere/e.py", WITH_ELIF)?;
    let rules = ELIF.replace("src/**/*.py", "**/*.py");
    project.rules(&rules)?;
    let out = project.check(&["--config", "other.toml"])?;
    assert_eq!(out.stdout, found("elsewhere/e.py:3:1", "t.elif", "no elif", 1, 0));
    Ok(())
}

#[test]
fn walk_missing_root_is_a_fault_naming_it() -> Outcome {
    let project = Project::with_rules("[\"nowhere\"]", ELIF)?;
    let out = project.check(&[])?;
    assert_eq!(out.code, 2);
    assert_eq!(out.stderr.lines().count(), 1, "{}", out.stderr);
    assert!(out.stderr.contains("nowhere"), "{}", out.stderr);
    Ok(())
}

#[test]
fn walk_no_configuration_is_exit_2() -> Outcome {
    let project = Project::new()?;
    project.file("src/a.py", WITH_ELIF)?;
    let out = project.check(&[])?;
    assert_eq!(out.code, 2);
    assert_eq!(out.stdout, "");
    assert!(out.stderr.contains("no law.toml"), "{}", out.stderr);
    Ok(())
}

#[test]
fn walk_builtin_eager_resolves_with_no_directory() -> Outcome {
    let project = Project::new()?;
    project.config("roots = [\"src\"]\ncatalogues = [\"eager\"]\n")?;
    project.file("src/pkg/a.py", "\"\"\"d\"\"\"\n")?;
    let out = project.check(&["--select", "ost.privates-first"])?;
    assert_eq!(out.stderr, "");
    assert_eq!((out.code, out.stdout.as_str()), (0, "All checks passed.\n"));
    Ok(())
}

#[test]
fn walk_unknown_builtin_name_is_an_unknown_catalogue_fault() -> Outcome {
    let project = Project::new()?;
    project.config("roots = [\"src\"]\ncatalogues = [\"nowhere\"]\n")?;
    let out = project.check(&[])?;
    assert_eq!(out.code, 2);
    assert_eq!(out.stdout, "");
    assert!(out.stderr.contains("nowhere"), "{}", out.stderr);
    Ok(())
}
