mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::thread::sleep;
use std::time::Duration;

use common::{ELIF, Outcome, Project, WITH_ELIF, passed};

const SWAP: &str = "- id: t.swap
  message: use new
  scope: {include: [\"src/**/*.py\"]}
  check:
    engine: pattern
    rule: {pattern: \"old($A)\"}
    fix: \"new($A)\"
  fix: {safety: safe}
";

const UNSAFE_DROP: &str = "- id: t.drop
  message: drop it
  scope: {include: [\"src/**/*.py\"]}
  check:
    engine: pattern
    rule: {pattern: \"drop($A)\"}
    fix: \"$A\"
  fix: {safety: unsafe}
";

const GROW: &str = "- id: t.grow
  message: grows
  scope: {include: [\"src/**/*.py\"]}
  check:
    engine: pattern
    rule: {pattern: \"old($A)\"}
    fix: \"old(old($A))\"
  fix: {safety: safe}
";

fn swap_project() -> Result<Project, Box<dyn std::error::Error>> {
    Project::with_rules("[\"src\"]", SWAP)
}

fn bytes(project: &Project, relative: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(fs::read(project.path().join(relative))?)
}

#[test]
fn fix_safe_fix_is_applied_and_the_file_is_byte_exact() -> Outcome {
    let project = swap_project()?;
    project.file("src/a.py", "x = 1\nold(a.b)\ny = 2\n")?;
    let out = project.check(&["--fix"])?;
    assert_eq!(out.stdout, "Fixed 1 finding.\nAll checks passed.\n");
    assert_eq!(out.stderr, "");
    assert_eq!(out.code, 0);
    assert_eq!(bytes(&project, "src/a.py")?, b"x = 1\nnew(a.b)\ny = 2\n");
    Ok(())
}

#[test]
fn fix_count_uses_the_plural_beyond_one() -> Outcome {
    let project = swap_project()?;
    project.file("src/a.py", "old(1)\nold(2)\n")?;
    let out = project.check(&["--fix"])?;
    assert_eq!(out.stdout, "Fixed 2 findings.\nAll checks passed.\n");
    assert_eq!(bytes(&project, "src/a.py")?, b"new(1)\nnew(2)\n");
    Ok(())
}

#[test]
fn fix_without_the_flag_never_writes() -> Outcome {
    let project = swap_project()?;
    project.file("src/a.py", "old(1)\n")?;
    let out = project.check(&[])?;
    assert_eq!(out.code, 1);
    assert_eq!(bytes(&project, "src/a.py")?, b"old(1)\n");
    Ok(())
}

#[test]
fn fix_unsafe_fix_is_untouched_without_the_flag_and_applied_with_it() -> Outcome {
    let project = Project::with_rules("[\"src\"]", UNSAFE_DROP)?;
    project.file("src/a.py", "drop(1)\n")?;
    let kept = project.check(&["--fix"])?;
    assert_eq!(
        kept.stdout,
        "src/a.py:1:1: t.drop drop it\n\nFound 1 finding (1 error, 0 warnings).\n"
    );
    assert_eq!(kept.code, 1);
    assert_eq!(bytes(&project, "src/a.py")?, b"drop(1)\n");
    let taken = project.check(&["--fix", "--unsafe-fixes"])?;
    assert_eq!(taken.stdout, "Fixed 1 finding.\nAll checks passed.\n");
    assert_eq!(taken.code, 0);
    assert_eq!(bytes(&project, "src/a.py")?, b"1\n");
    Ok(())
}

#[test]
fn fix_unsafe_flag_without_fix_is_a_usage_error() -> Outcome {
    let project = swap_project()?;
    project.file("src/a.py", "old(1)\n")?;
    let out = project.check(&["--unsafe-fixes"])?;
    assert_eq!(out.code, 2);
    assert_eq!(out.stdout, "");
    assert!(out.stderr.contains("--fix"), "{}", out.stderr);
    assert_eq!(bytes(&project, "src/a.py")?, b"old(1)\n");
    Ok(())
}

#[test]
fn fix_overlapping_fixes_land_one_per_pass_and_both_after_the_loop() -> Outcome {
    let project = swap_project()?;
    project.file("src/a.py", "old(old(1))\n")?;
    let out = project.check(&["--fix"])?;
    assert_eq!(out.stdout, "Fixed 2 findings.\nAll checks passed.\n");
    assert_eq!(out.code, 0);
    assert_eq!(bytes(&project, "src/a.py")?, b"new(new(1))\n");
    Ok(())
}

#[test]
fn fix_loop_stops_at_ten_passes_when_a_fix_keeps_producing_findings() -> Outcome {
    let project = Project::with_rules("[\"src\"]", GROW)?;
    project.file("src/a.py", "old(1)\n")?;
    let out = project.check(&["--fix"])?;
    assert_eq!(out.code, 1);
    assert!(
        out.stdout
            .ends_with("\nFixed 10 findings.\nFound 11 findings (11 errors, 0 warnings).\n"),
        "{}",
        out.stdout
    );
    assert_eq!(
        bytes(&project, "src/a.py")?
            .windows(4)
            .filter(|w| *w == b"old(")
            .count(),
        11
    );
    Ok(())
}

#[test]
fn fix_second_run_changes_nothing_and_keeps_the_mtime() -> Outcome {
    let project = swap_project()?;
    project.file("src/a.py", "old(1)\n")?;
    project.file("src/b.py", "x = 1\n")?;
    assert_eq!(project.check(&["--fix"])?.code, 0);
    let path = project.path().join("src/a.py");
    let before = (fs::read(&path)?, fs::metadata(&path)?.modified()?);
    sleep(Duration::from_millis(30));
    let again = project.check(&["--fix"])?;
    assert_eq!(again.stdout, "All checks passed.\n");
    assert_eq!(again.code, 0);
    assert_eq!((fs::read(&path)?, fs::metadata(&path)?.modified()?), before);
    Ok(())
}

#[test]
fn fix_clean_files_are_never_rewritten() -> Outcome {
    let project = swap_project()?;
    project.file("src/a.py", "old(1)\n")?;
    project.file("src/clean.py", "x = 1\n")?;
    let clean = project.path().join("src/clean.py");
    let before = fs::metadata(&clean)?.modified()?;
    sleep(Duration::from_millis(30));
    project.check(&["--fix"])?;
    assert_eq!(fs::metadata(&clean)?.modified()?, before);
    Ok(())
}

#[test]
fn fix_only_touches_the_paths_given() -> Outcome {
    let project = swap_project()?;
    project.file("src/a.py", "old(1)\n")?;
    project.file("src/b.py", "old(2)\n")?;
    let out = project.check(&["--fix", "src/a.py"])?;
    assert_eq!(out.stdout, "Fixed 1 finding.\nAll checks passed.\n");
    assert_eq!(bytes(&project, "src/a.py")?, b"new(1)\n");
    assert_eq!(bytes(&project, "src/b.py")?, b"old(2)\n");
    Ok(())
}

#[test]
fn fix_crlf_endings_survive() -> Outcome {
    let project = swap_project()?;
    project.file("src/a.py", "x = 1\r\nold(a)\r\ny = 2\r\n")?;
    let out = project.check(&["--fix"])?;
    assert_eq!(out.code, 0);
    assert_eq!(bytes(&project, "src/a.py")?, b"x = 1\r\nnew(a)\r\ny = 2\r\n");
    Ok(())
}

#[test]
fn fix_multibyte_text_before_the_edit_keeps_the_edit_in_place() -> Outcome {
    let project = swap_project()?;
    project.file("src/a.py", "s = 'caf\u{e9} \u{1f600} \u{4e16}\u{754c}'; old(\u{e9})\n")?;
    let out = project.check(&["--fix"])?;
    assert_eq!(out.code, 0, "{out:?}");
    assert_eq!(
        String::from_utf8(bytes(&project, "src/a.py")?)?,
        "s = 'caf\u{e9} \u{1f600} \u{4e16}\u{754c}'; new(\u{e9})\n"
    );
    Ok(())
}

#[test]
fn fix_finding_without_a_fix_stays_reported() -> Outcome {
    let project = Project::with_rules("[\"src\"]", ELIF)?;
    project.file("src/a.py", WITH_ELIF)?;
    let out = project.check(&["--fix"])?;
    assert_eq!(out.code, 1);
    assert!(out.stdout.contains("src/a.py:3:1: t.elif no elif\n"), "{}", out.stdout);
    assert!(!out.stdout.contains("Fixed"), "{}", out.stdout);
    assert_eq!(bytes(&project, "src/a.py")?, WITH_ELIF.as_bytes());
    Ok(())
}

#[test]
fn fix_the_exit_code_comes_from_the_final_report() -> Outcome {
    let both = format!("{SWAP}{ELIF}");
    let project = Project::with_rules("[\"src\"]", &both)?;
    project.file("src/a.py", &format!("old(1)\n{WITH_ELIF}"))?;
    let out = project.check(&["--fix"])?;
    assert_eq!(out.code, 1);
    assert!(out.stdout.contains("t.elif"), "{}", out.stdout);
    assert!(!out.stdout.contains("t.swap"), "{}", out.stdout);
    Ok(())
}

#[test]
fn fix_json_report_is_the_last_check_and_stays_valid_json() -> Outcome {
    let project = swap_project()?;
    project.file("src/a.py", "old(1)\n")?;
    let report = project.json(&["--fix"])?;
    assert_eq!(report["findings"], serde_json::json!([]));
    assert_eq!(bytes(&project, "src/a.py")?, b"new(1)\n");
    Ok(())
}

#[test]
fn fix_unwritable_directory_is_a_fault_not_a_panic() -> Outcome {
    let project = swap_project()?;
    project.file("src/pkg/a.py", "old(1)\n")?;
    let dir = project.path().join("src/pkg");
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o555))?;
    let out = project.check(&["--fix"])?;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755))?;
    assert_eq!(out.code, 2, "{out:?}");
    assert!(!out.stderr.contains("panicked"), "{}", out.stderr);
    assert!(out.stderr.contains("src/pkg/a.py"), "{}", out.stderr);
    assert!(
        out.stdout.ends_with("1 fault: the run is incomplete.\n"),
        "{}",
        out.stdout
    );
    assert_eq!(bytes(&project, "src/pkg/a.py")?, b"old(1)\n");
    Ok(())
}

#[test]
fn fix_read_only_file_is_a_fault_not_a_panic() -> Outcome {
    let project = swap_project()?;
    let path = project.file("src/a.py", "old(1)\n")?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o444))?;
    let out = project.check(&["--fix"])?;
    assert_eq!(out.code, 2, "{out:?}");
    assert!(!out.stderr.contains("panicked"), "{}", out.stderr);
    assert_eq!(bytes(&project, "src/a.py")?, b"old(1)\n");
    Ok(())
}

#[test]
fn fix_clean_project_with_the_flag_passes() -> Outcome {
    let project = swap_project()?;
    project.file("src/a.py", "x = 1\n")?;
    let out = project.check(&["--fix"])?;
    assert!(passed(&out), "{out:?}");
    Ok(())
}
