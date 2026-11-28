mod common;

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use common::{Outcome, finding, read, relocate, replace, safe, tree};
use law_fix::{Fault, apply, patch, project};
use law_model::Safety;

fn exists(dir: &Path, name: &str) -> bool {
    dir.join(name).exists()
}

fn walk(root: &Path, dir: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            found.extend(walk(root, &path)?);
        } else {
            found.push(path.strip_prefix(root)?.to_string_lossy().into_owned());
        }
    }
    Ok(found)
}

fn names(dir: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut all = walk(dir, dir)?;
    all.sort();
    Ok(all)
}

#[test]
fn move_renames_the_file_and_keeps_its_bytes() -> Outcome {
    let dir = tree(&[("src/a.py", b"x = 1\r\n\xc3\xa9\n")])?;
    let moved = relocate("t.m", "src/a.py", "src/a/engine.py", &[])?;
    let applied = apply(dir.path(), std::slice::from_ref(&moved), Safety::Unsafe);
    assert_eq!(applied.fixed, [moved]);
    assert!(applied.faults.is_empty() && applied.skipped.is_empty());
    assert_eq!(fs::read(dir.path().join("src/a/engine.py"))?, b"x = 1\r\n\xc3\xa9\n");
    assert_eq!(names(dir.path())?, ["src/a/engine.py"]);
    Ok(())
}

#[test]
fn move_is_unsafe_and_waits_for_the_allowance() -> Outcome {
    let dir = tree(&[("a.py", b"x")])?;
    let moved = relocate("t.m", "a.py", "a/engine.py", &[])?;
    let applied = apply(dir.path(), &[moved], Safety::Safe);
    assert!(applied.fixed.is_empty() && applied.faults.is_empty());
    assert_eq!(names(dir.path())?, ["a.py"]);
    Ok(())
}

#[test]
fn move_carries_the_edits_of_the_old_path_to_the_new_one() -> Outcome {
    let dir = tree(&[("a.py", b"import old\nx = 1\n")])?;
    let moved = relocate("t.m", "a.py", "a/engine.py", &[replace("a.py", 7, 10, "new")])?;
    let applied = apply(dir.path(), &[moved], Safety::Unsafe);
    assert!(applied.faults.is_empty(), "{:?}", applied.faults);
    assert_eq!(read(dir.path(), "a/engine.py")?, "import new\nx = 1\n");
    assert!(!exists(dir.path(), "a.py"));
    Ok(())
}

#[test]
fn move_and_the_edit_of_another_finding_in_the_same_file_land_together() -> Outcome {
    let dir = tree(&[("a.py", b"import old\nx = 1\n")])?;
    let moved = relocate("t.m", "a.py", "a/engine.py", &[])?;
    let edit = safe("t.e", "a.py", 7, 10, "new")?;
    let applied = apply(dir.path(), &[moved, edit], Safety::Unsafe);
    assert_eq!(applied.fixed.len(), 2, "{:?}", applied.faults);
    assert_eq!(read(dir.path(), "a/engine.py")?, "import new\nx = 1\n");
    assert!(!exists(dir.path(), "a.py"));
    Ok(())
}

#[test]
fn move_edits_other_files_in_the_same_stroke() -> Outcome {
    let dir = tree(&[("src/a.py", b"x"), ("src/b.py", b"from a import x\n")])?;
    let moved = relocate(
        "t.m",
        "src/a.py",
        "src/a/engine.py",
        &[replace("src/b.py", 5, 6, "a.engine")],
    )?;
    let applied = apply(dir.path(), &[moved], Safety::Unsafe);
    assert_eq!(applied.fixed.len(), 1, "{:?}", applied.faults);
    assert_eq!(read(dir.path(), "src/b.py")?, "from a.engine import x\n");
    assert_eq!(read(dir.path(), "src/a/engine.py")?, "x");
    Ok(())
}

#[test]
fn move_onto_a_file_that_exists_is_a_fault_and_writes_nothing_of_the_fix() -> Outcome {
    let dir = tree(&[("a.py", b"aaa"), ("b.py", b"bbb"), ("a/engine.py", b"mine")])?;
    let moved = relocate("t.m", "a.py", "a/engine.py", &[replace("b.py", 0, 1, "X")])?;
    let applied = apply(dir.path(), &[moved], Safety::Unsafe);
    assert!(applied.fixed.is_empty());
    assert!(
        matches!(applied.faults.as_slice(), [Fault::Occupied { path }] if path == Path::new("a/engine.py")),
        "{:?}",
        applied.faults
    );
    assert_eq!(read(dir.path(), "a.py")?, "aaa");
    assert_eq!(read(dir.path(), "b.py")?, "bbb");
    assert_eq!(read(dir.path(), "a/engine.py")?, "mine");
    assert_eq!(names(dir.path())?, ["a.py", "a/engine.py", "b.py"]);
    Ok(())
}

#[test]
fn move_of_a_read_only_file_is_a_fault_and_writes_nothing_of_the_fix() -> Outcome {
    let dir = tree(&[("a.py", b"aaa"), ("b.py", b"bbb")])?;
    fs::set_permissions(dir.path().join("a.py"), fs::Permissions::from_mode(0o444))?;
    let moved = relocate("t.m", "a.py", "a/engine.py", &[replace("b.py", 0, 1, "X")])?;
    let applied = apply(dir.path(), &[moved], Safety::Unsafe);
    assert!(applied.fixed.is_empty());
    assert!(
        matches!(applied.faults.as_slice(), [Fault::Io { .. }]),
        "{:?}",
        applied.faults
    );
    assert_eq!(read(dir.path(), "b.py")?, "bbb");
    assert_eq!(names(dir.path())?, ["a.py", "b.py"]);
    Ok(())
}

#[test]
fn move_out_of_an_unwritable_directory_is_a_fault_and_leaves_the_tree_as_it_was() -> Outcome {
    let dir = tree(&[("sub/a.py", b"aaa"), ("b.py", b"bbb")])?;
    let sub = dir.path().join("sub");
    fs::set_permissions(&sub, fs::Permissions::from_mode(0o555))?;
    let moved = relocate("t.m", "sub/a.py", "other/a/engine.py", &[replace("b.py", 0, 1, "X")])?;
    let applied = apply(dir.path(), &[moved], Safety::Unsafe);
    fs::set_permissions(&sub, fs::Permissions::from_mode(0o755))?;
    assert!(applied.fixed.is_empty());
    assert!(
        matches!(applied.faults.as_slice(), [Fault::Io { .. }]),
        "{:?}",
        applied.faults
    );
    assert_eq!(read(dir.path(), "b.py")?, "bbb");
    assert_eq!(names(dir.path())?, ["b.py", "sub/a.py"]);
    assert!(
        !exists(dir.path(), "other"),
        "a directory the failed move made was left behind"
    );
    Ok(())
}

#[test]
fn move_that_fails_does_not_stop_the_other_findings() -> Outcome {
    let dir = tree(&[("a.py", b"aaa"), ("b.py", b"bbb"), ("a/engine.py", b"mine")])?;
    let blocked = relocate("t.m", "a.py", "a/engine.py", &[])?;
    let good = safe("t.r", "b.py", 0, 1, "X")?;
    let applied = apply(dir.path(), &[blocked, good.clone()], Safety::Unsafe);
    assert_eq!(applied.fixed, [good]);
    assert_eq!(read(dir.path(), "b.py")?, "Xbb");
    assert_eq!(applied.faults.len(), 1);
    Ok(())
}

#[test]
fn move_two_findings_of_one_file_take_the_first_and_skip_the_other() -> Outcome {
    let dir = tree(&[("a.py", b"aaa")])?;
    let one = relocate("t.m", "a.py", "a/engine.py", &[])?;
    let two = relocate("t.n", "a.py", "a/driver.py", &[])?;
    let applied = apply(dir.path(), &[one, two], Safety::Unsafe);
    assert_eq!((applied.fixed.len(), applied.skipped.len()), (1, 1));
    assert_eq!(names(dir.path())?.len(), 1);
    Ok(())
}

#[test]
fn move_keeps_the_mode_and_leaves_no_temporary_file() -> Outcome {
    let dir = tree(&[("a.py", b"abc")])?;
    fs::set_permissions(dir.path().join("a.py"), fs::Permissions::from_mode(0o750))?;
    let applied = apply(
        dir.path(),
        &[relocate("t.m", "a.py", "a/engine.py", &[])?],
        Safety::Unsafe,
    );
    assert!(applied.faults.is_empty());
    assert_eq!(
        fs::metadata(dir.path().join("a/engine.py"))?.permissions().mode() & 0o777,
        0o750
    );
    assert_eq!(names(dir.path())?, ["a/engine.py"]);
    assert_eq!(fs::read_dir(dir.path())?.count(), 1);
    Ok(())
}

#[test]
fn move_edit_beyond_the_file_is_a_range_fault_and_nothing_moves() -> Outcome {
    let dir = tree(&[("a.py", b"abc")])?;
    let moved = relocate("t.m", "a.py", "a/engine.py", &[replace("a.py", 1, 40, "x")])?;
    let applied = apply(dir.path(), &[moved], Safety::Unsafe);
    assert!(
        matches!(applied.faults.as_slice(), [Fault::Range { .. }]),
        "{:?}",
        applied.faults
    );
    assert_eq!(names(dir.path())?, ["a.py"]);
    Ok(())
}

#[test]
fn project_moves_and_edits_in_memory() -> Outcome {
    let files: BTreeMap<PathBuf, String> = [
        (PathBuf::from("src/a.py"), "x = 1\n".to_owned()),
        (PathBuf::from("src/b.py"), "from a import x\n".to_owned()),
    ]
    .into();
    let moved = relocate(
        "t.m",
        "src/a.py",
        "src/a/engine.py",
        &[replace("src/b.py", 5, 6, "a.engine")],
    )?;
    let (after, applied) = project(&files, &[moved], Safety::Unsafe);
    assert!(applied.faults.is_empty(), "{:?}", applied.faults);
    assert_eq!(
        after,
        BTreeMap::from([
            (PathBuf::from("src/a/engine.py"), "x = 1\n".to_owned()),
            (PathBuf::from("src/b.py"), "from a.engine import x\n".to_owned()),
        ])
    );
    Ok(())
}

#[test]
fn project_move_onto_an_existing_path_is_a_fault_and_the_files_are_unchanged() -> Outcome {
    let files: BTreeMap<PathBuf, String> = [
        (PathBuf::from("a.py"), "x".to_owned()),
        (PathBuf::from("a/engine.py"), "y".to_owned()),
    ]
    .into();
    let (after, applied) = project(&files, &[relocate("t.m", "a.py", "a/engine.py", &[])?], Safety::Unsafe);
    assert_eq!(after, files);
    assert!(
        matches!(applied.faults.as_slice(), [Fault::Occupied { .. }]),
        "{:?}",
        applied.faults
    );
    Ok(())
}

#[test]
fn patch_takes_no_move() -> Outcome {
    let moved = relocate("t.m", "a.py", "a/engine.py", &[replace("a.py", 0, 1, "X")])?;
    let (text, applied) = patch(Path::new("a.py"), "abc", &[moved], Safety::Unsafe);
    assert_eq!(text, "abc");
    assert!(applied.fixed.is_empty());
    let other = finding("t.o", "a.py", "unsafe", &[replace("a.py", 0, 1, "X")])?;
    let (text, _) = patch(Path::new("a.py"), "abc", &[other], Safety::Unsafe);
    assert_eq!(text, "Xbc");
    Ok(())
}
