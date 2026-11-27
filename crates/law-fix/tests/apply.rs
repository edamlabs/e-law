mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use common::{Outcome, finding, read, replace, safe, tree};
use law_fix::{Fault, apply};
use law_model::Safety;

#[test]
fn apply_safe_edit_leaves_the_file_byte_exact() -> Outcome {
    let dir = tree(&[("a.py", b"x = 1\nelif y\nz = 2\n")])?;
    let findings = [safe("t.r", "a.py", 6, 10, "else")?];
    let applied = apply(dir.path(), &findings, Safety::Safe);
    assert_eq!(read(dir.path(), "a.py")?, "x = 1\nelse y\nz = 2\n");
    assert_eq!(applied.fixed, findings);
    assert!(applied.skipped.is_empty() && applied.faults.is_empty());
    Ok(())
}

#[test]
fn apply_unsafe_edit_is_untouched_without_the_allowance() -> Outcome {
    let dir = tree(&[("a.py", b"abc\n")])?;
    let findings = [finding("t.r", "a.py", "unsafe", &[replace("a.py", 0, 3, "xyz")])?];
    let applied = apply(dir.path(), &findings, Safety::Safe);
    assert_eq!(read(dir.path(), "a.py")?, "abc\n");
    assert!(applied.fixed.is_empty() && applied.skipped.is_empty() && applied.faults.is_empty());
    let applied = apply(dir.path(), &findings, Safety::Unsafe);
    assert_eq!(read(dir.path(), "a.py")?, "xyz\n");
    assert_eq!(applied.fixed.len(), 1);
    Ok(())
}

#[test]
fn apply_unsafe_allowance_takes_the_safe_ones_too() -> Outcome {
    let dir = tree(&[("a.py", b"abcdef")])?;
    let findings = [
        safe("t.r", "a.py", 0, 1, "A")?,
        finding("t.u", "a.py", "unsafe", &[replace("a.py", 3, 4, "D")])?,
    ];
    let applied = apply(dir.path(), &findings, Safety::Unsafe);
    assert_eq!(read(dir.path(), "a.py")?, "AbcDef");
    assert_eq!(applied.fixed.len(), 2);
    Ok(())
}

#[test]
fn apply_edits_of_one_file_land_back_to_front_whatever_their_length() -> Outcome {
    let dir = tree(&[("a.py", b"aa bb cc dd")])?;
    let findings = [
        safe("t.r", "a.py", 0, 2, "LONGER")?,
        safe("t.r", "a.py", 3, 5, "")?,
        safe("t.r", "a.py", 9, 11, "d")?,
    ];
    let applied = apply(dir.path(), &findings, Safety::Safe);
    assert_eq!(read(dir.path(), "a.py")?, "LONGER  cc d");
    assert_eq!(applied.fixed.len(), 3);
    Ok(())
}

#[test]
fn apply_overlapping_edits_take_the_first_by_position_and_skip_the_other() -> Outcome {
    let dir = tree(&[("a.py", b"0123456789")])?;
    let first = safe("t.a", "a.py", 2, 6, "AAAA")?;
    let second = safe("t.b", "a.py", 4, 8, "BBBB")?;
    let applied = apply(dir.path(), &[second.clone(), first.clone()], Safety::Safe);
    assert_eq!(read(dir.path(), "a.py")?, "01AAAA6789");
    assert_eq!(applied.fixed, [first]);
    assert_eq!(applied.skipped, [second]);
    Ok(())
}

#[test]
fn apply_same_start_is_a_conflict() -> Outcome {
    let dir = tree(&[("a.py", b"0123456789")])?;
    let applied = apply(
        dir.path(),
        &[safe("t.a", "a.py", 2, 4, "A")?, safe("t.b", "a.py", 2, 6, "B")?],
        Safety::Safe,
    );
    assert_eq!(applied.fixed.len(), 1);
    assert_eq!(applied.skipped.len(), 1);
    Ok(())
}

#[test]
fn apply_touching_edits_do_not_conflict() -> Outcome {
    let dir = tree(&[("a.py", b"0123456789")])?;
    let applied = apply(
        dir.path(),
        &[safe("t.a", "a.py", 2, 4, "AA")?, safe("t.b", "a.py", 4, 6, "BB")?],
        Safety::Safe,
    );
    assert_eq!(read(dir.path(), "a.py")?, "01AABB6789");
    assert_eq!(applied.fixed.len(), 2);
    assert!(applied.skipped.is_empty());
    Ok(())
}

#[test]
fn apply_a_fix_is_all_or_nothing_across_its_edits() -> Outcome {
    let dir = tree(&[("a.py", b"0123456789"), ("b.py", b"abcdef")])?;
    let blocker = safe("t.a", "a.py", 4, 6, "XX")?;
    let pair = finding(
        "t.b",
        "b.py",
        "safe",
        &[replace("b.py", 0, 1, "Z"), replace("a.py", 5, 7, "YY")],
    )?;
    let applied = apply(dir.path(), &[blocker, pair.clone()], Safety::Safe);
    assert_eq!(read(dir.path(), "a.py")?, "0123XX6789");
    assert_eq!(read(dir.path(), "b.py")?, "abcdef");
    assert_eq!(applied.skipped, [pair]);
    Ok(())
}

#[test]
fn apply_a_fix_across_two_files_lands_in_both() -> Outcome {
    let dir = tree(&[("a.py", b"aaa"), ("b.py", b"bbb")])?;
    let pair = finding(
        "t.b",
        "a.py",
        "safe",
        &[replace("a.py", 0, 1, "A"), replace("b.py", 2, 3, "B")],
    )?;
    let applied = apply(dir.path(), &[pair], Safety::Safe);
    assert_eq!(
        (read(dir.path(), "a.py")?.as_str(), read(dir.path(), "b.py")?.as_str()),
        ("Aaa", "bbB")
    );
    assert_eq!(applied.fixed.len(), 1);
    Ok(())
}

#[test]
fn apply_multibyte_text_before_the_edit_keeps_byte_offsets() -> Outcome {
    let text = "caf\u{e9} \u{1f600} x = 1\n";
    let start = text.find("x = 1").ok_or("no needle")?;
    let dir = tree(&[("a.py", text.as_bytes())])?;
    let applied = apply(
        dir.path(),
        &[safe("t.r", "a.py", start, start + 5, "y = 2")?],
        Safety::Safe,
    );
    assert_eq!(read(dir.path(), "a.py")?, "caf\u{e9} \u{1f600} y = 2\n");
    assert!(applied.faults.is_empty());
    Ok(())
}

#[test]
fn apply_crlf_line_endings_survive() -> Outcome {
    let dir = tree(&[("a.py", b"a = 1\r\nelif b\r\nc = 3\r\n")])?;
    let applied = apply(dir.path(), &[safe("t.r", "a.py", 7, 11, "else")?], Safety::Safe);
    assert!(applied.faults.is_empty());
    assert_eq!(fs::read(dir.path().join("a.py"))?, b"a = 1\r\nelse b\r\nc = 3\r\n");
    Ok(())
}

#[test]
fn apply_edit_beyond_the_file_is_a_range_fault_and_the_file_is_left_alone() -> Outcome {
    let dir = tree(&[("a.py", b"abc")])?;
    let applied = apply(dir.path(), &[safe("t.r", "a.py", 2, 99, "x")?], Safety::Safe);
    assert_eq!(read(dir.path(), "a.py")?, "abc");
    assert!(applied.fixed.is_empty());
    assert!(
        matches!(applied.faults.as_slice(), [Fault::Range { .. }]),
        "{:?}",
        applied.faults
    );
    Ok(())
}

#[test]
fn apply_edit_splitting_a_character_is_a_fault_and_the_file_is_left_alone() -> Outcome {
    let dir = tree(&[("a.py", "caf\u{e9}\n".as_bytes())])?;
    let applied = apply(dir.path(), &[safe("t.r", "a.py", 4, 5, "x")?], Safety::Safe);
    assert_eq!(fs::read(dir.path().join("a.py"))?, "caf\u{e9}\n".as_bytes());
    assert!(applied.fixed.is_empty());
    assert_eq!(applied.faults.len(), 1);
    Ok(())
}

#[test]
fn apply_a_failed_file_does_not_stop_the_others() -> Outcome {
    let dir = tree(&[("a.py", b"abc"), ("b.py", b"abc")])?;
    let good = safe("t.r", "b.py", 0, 1, "X")?;
    let applied = apply(
        dir.path(),
        &[safe("t.r", "a.py", 0, 99, "x")?, good.clone()],
        Safety::Safe,
    );
    assert_eq!(read(dir.path(), "b.py")?, "Xbc");
    assert_eq!(applied.fixed, [good]);
    assert_eq!(applied.faults.len(), 1);
    Ok(())
}

#[test]
fn apply_missing_file_is_an_io_fault() -> Outcome {
    let dir = tree(&[])?;
    let applied = apply(dir.path(), &[safe("t.r", "ghost.py", 0, 1, "x")?], Safety::Safe);
    assert!(applied.fixed.is_empty());
    assert!(
        matches!(applied.faults.as_slice(), [Fault::Io { .. }]),
        "{:?}",
        applied.faults
    );
    Ok(())
}

#[test]
fn apply_create_and_move_are_unsupported_faults_and_touch_nothing() -> Outcome {
    let dir = tree(&[("a.py", b"abc")])?;
    let create = finding(
        "t.c",
        "a.py",
        "safe",
        &[serde_json::json!({"edit": "create", "path": "n.py", "text": "x"})],
    )?;
    let moved = finding(
        "t.m",
        "a.py",
        "safe",
        &[serde_json::json!({"edit": "move", "from": "a.py", "to": "b.py"})],
    )?;
    let applied = apply(dir.path(), &[create, moved], Safety::Safe);
    assert_eq!(applied.faults.len(), 2);
    assert!(
        applied
            .faults
            .iter()
            .all(|fault| matches!(fault, Fault::Unsupported { .. }))
    );
    assert!(applied.fixed.is_empty());
    assert_eq!(read(dir.path(), "a.py")?, "abc");
    assert!(!dir.path().join("n.py").exists() && !dir.path().join("b.py").exists());
    Ok(())
}

#[test]
fn apply_nothing_to_fix_touches_nothing() -> Outcome {
    let dir = tree(&[("a.py", b"abc")])?;
    let applied = apply(dir.path(), &[], Safety::Unsafe);
    assert!(applied.fixed.is_empty() && applied.skipped.is_empty() && applied.faults.is_empty());
    Ok(())
}

#[test]
fn apply_an_edit_that_changes_nothing_does_not_rewrite_the_file() -> Outcome {
    let dir = tree(&[("a.py", b"abc")])?;
    let path = dir.path().join("a.py");
    let before = fs::metadata(&path)?.modified()?;
    std::thread::sleep(Duration::from_millis(20));
    let applied = apply(dir.path(), &[safe("t.r", "a.py", 0, 1, "a")?], Safety::Safe);
    assert_eq!(fs::metadata(&path)?.modified()?, before);
    assert_eq!(applied.fixed.len(), 1);
    Ok(())
}

#[test]
fn apply_keeps_the_file_mode_and_leaves_no_temporary_file() -> Outcome {
    let dir = tree(&[("a.py", b"abc")])?;
    let path = dir.path().join("a.py");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o750))?;
    let applied = apply(dir.path(), &[safe("t.r", "a.py", 0, 1, "X")?], Safety::Safe);
    assert!(applied.faults.is_empty());
    assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o777, 0o750);
    let names: Vec<_> = fs::read_dir(dir.path())?
        .map(|entry| entry.map(|e| e.file_name()))
        .collect::<Result<_, _>>()?;
    assert_eq!(names, ["a.py"]);
    Ok(())
}

#[test]
fn apply_unwritable_directory_is_an_io_fault_not_a_panic() -> Outcome {
    let dir = tree(&[("sub/a.py", b"abc")])?;
    let sub = dir.path().join("sub");
    fs::set_permissions(&sub, fs::Permissions::from_mode(0o555))?;
    let applied = apply(dir.path(), &[safe("t.r", "sub/a.py", 0, 1, "X")?], Safety::Safe);
    fs::set_permissions(&sub, fs::Permissions::from_mode(0o755))?;
    assert_eq!(read(dir.path(), "sub/a.py")?, "abc");
    assert!(applied.fixed.is_empty());
    assert!(
        matches!(applied.faults.as_slice(), [Fault::Io { .. }]),
        "{:?}",
        applied.faults
    );
    Ok(())
}

#[test]
fn apply_unreadable_file_is_an_io_fault_not_a_panic() -> Outcome {
    let dir = tree(&[("a.py", b"abc")])?;
    let path = dir.path().join("a.py");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o000))?;
    let applied = apply(dir.path(), &[safe("t.r", "a.py", 0, 1, "X")?], Safety::Safe);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644))?;
    assert_eq!(read(dir.path(), "a.py")?, "abc");
    assert!(
        matches!(applied.faults.as_slice(), [Fault::Io { .. }]),
        "{:?}",
        applied.faults
    );
    Ok(())
}

#[test]
fn apply_read_only_file_is_not_silently_overwritten() -> Outcome {
    let dir = tree(&[("a.py", b"abc")])?;
    let path = dir.path().join("a.py");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o444))?;
    let applied = apply(dir.path(), &[safe("t.r", "a.py", 0, 1, "X")?], Safety::Safe);
    assert_eq!(read(dir.path(), "a.py")?, "abc", "a read-only file was rewritten");
    assert_eq!(applied.faults.len(), 1);
    assert!(applied.fixed.is_empty());
    Ok(())
}
