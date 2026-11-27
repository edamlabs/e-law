mod common;

use std::path::Path;

use common::{Outcome, finding, replace, safe};
use law_fix::{Fault, patch};
use law_model::Safety;

#[test]
fn patch_edits_the_text_in_memory() -> Outcome {
    let findings = [safe("t.r", "a.py", 6, 10, "else")?];
    let (text, applied) = patch(Path::new("a.py"), "x = 1\nelif y\n", &findings, Safety::Safe);
    assert_eq!(text, "x = 1\nelse y\n");
    assert_eq!(applied.fixed, findings);
    assert!(applied.faults.is_empty());
    Ok(())
}

#[test]
fn patch_unsafe_edit_is_left_without_the_allowance() -> Outcome {
    let findings = [finding("t.u", "a.py", "unsafe", &[replace("a.py", 0, 1, "X")])?];
    let (text, applied) = patch(Path::new("a.py"), "abc", &findings, Safety::Safe);
    assert_eq!(text, "abc");
    assert!(applied.fixed.is_empty());
    let (text, _) = patch(Path::new("a.py"), "abc", &findings, Safety::Unsafe);
    assert_eq!(text, "Xbc");
    Ok(())
}

#[test]
fn patch_overlap_takes_the_first_and_skips_the_other() -> Outcome {
    let findings = [safe("t.a", "a.py", 0, 2, "AA")?, safe("t.b", "a.py", 1, 3, "BB")?];
    let (text, applied) = patch(Path::new("a.py"), "abcd", &findings, Safety::Safe);
    assert_eq!(text, "AAcd");
    assert_eq!(applied.skipped.len(), 1);
    Ok(())
}

#[test]
fn patch_takes_no_edit_of_another_file() -> Outcome {
    let findings = [safe("t.a", "b.py", 0, 1, "X")?];
    let (text, applied) = patch(Path::new("a.py"), "abc", &findings, Safety::Safe);
    assert_eq!(text, "abc");
    assert!(applied.fixed.is_empty());
    Ok(())
}

#[test]
fn patch_out_of_range_edit_is_a_range_fault_and_the_text_is_returned_whole() -> Outcome {
    let (text, applied) = patch(
        Path::new("a.py"),
        "abc",
        &[safe("t.r", "a.py", 1, 40, "x")?],
        Safety::Safe,
    );
    assert_eq!(text, "abc");
    assert!(applied.fixed.is_empty());
    assert!(
        matches!(applied.faults.as_slice(), [Fault::Range { .. }]),
        "{:?}",
        applied.faults
    );
    Ok(())
}

#[test]
fn patch_edit_splitting_a_character_is_a_range_fault() -> Outcome {
    let (text, applied) = patch(
        Path::new("a.py"),
        "caf\u{e9}",
        &[safe("t.r", "a.py", 4, 5, "x")?],
        Safety::Safe,
    );
    assert_eq!(text, "caf\u{e9}");
    assert!(
        matches!(applied.faults.as_slice(), [Fault::Range { .. }]),
        "{:?}",
        applied.faults
    );
    Ok(())
}

#[test]
fn patch_multibyte_and_crlf_text_keeps_offsets_and_endings() -> Outcome {
    let text = "\u{e9}\u{e9}\r\nelif\r\n";
    let (patched, _) = patch(
        Path::new("a.py"),
        text,
        &[safe("t.r", "a.py", 6, 10, "else")?],
        Safety::Safe,
    );
    assert_eq!(patched, "\u{e9}\u{e9}\r\nelse\r\n");
    Ok(())
}
