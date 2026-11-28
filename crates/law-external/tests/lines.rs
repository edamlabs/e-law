mod common;

use law_external::Lines;
use law_model::Position;

fn at(line: u32, column: u32) -> Position {
    Position { line, column }
}

#[test]
fn lines_ascii_offsets_follow_row_and_column() {
    let lines = Lines::new("ab\ncde\n");
    assert_eq!(lines.offset(1, 1), Some(0));
    assert_eq!(lines.offset(1, 3), Some(2));
    assert_eq!(lines.offset(2, 1), Some(3));
    assert_eq!(lines.offset(2, 3), Some(5));
    assert_eq!(lines.offset(2, 4), Some(6));
}

#[test]
fn lines_columns_count_characters_not_bytes() {
    let text = "s = 'caf\u{e9} \u{1f600} \u{4e16}'; x\n";
    let lines = Lines::new(text);
    let x = text.find("x\n").map(|b| text[..b].chars().count() + 1);
    let column = u32::try_from(x.unwrap_or(0)).unwrap_or(0);
    assert_eq!(lines.offset(1, column), text.find("x\n"));
    assert_eq!(lines.offset(1, 8), Some(7));
    assert_eq!(lines.offset(1, 9), Some(8), "the two-byte e-acute starts here");
    assert_eq!(lines.offset(1, 10), Some(10), "after the two-byte e-acute");
    assert_eq!(lines.offset(1, 12), Some(15), "after the four-byte emoji");
}

#[test]
fn lines_crlf_is_one_break() {
    let lines = Lines::new("a\r\nbc\r\n");
    assert_eq!(lines.offset(2, 1), Some(3));
    assert_eq!(lines.offset(2, 3), Some(5));
    assert_eq!(lines.offset(3, 1), Some(7));
    assert_eq!(lines.offset(4, 1), None);
}

#[test]
fn lines_a_lone_cr_is_a_break() {
    let lines = Lines::new("a\rbc\rd");
    assert_eq!(lines.offset(2, 1), Some(2));
    assert_eq!(lines.offset(3, 1), Some(5));
    assert_eq!(lines.offset(3, 2), Some(6));
    assert_eq!(lines.offset(4, 1), None);
}

#[test]
fn lines_mixed_breaks_each_count_once() {
    let lines = Lines::new("a\nb\r\nc\rd");
    assert_eq!(
        [1, 2, 3, 4].map(|line| lines.offset(line, 1)),
        [Some(0), Some(2), Some(5), Some(7)]
    );
}

#[test]
fn lines_end_of_file_positions() {
    let with_newline = Lines::new("ab\n");
    assert_eq!(
        with_newline.offset(2, 1),
        Some(3),
        "the empty line after the last break"
    );
    let without = Lines::new("ab");
    assert_eq!(without.offset(1, 3), Some(2), "one past the last character");
    assert_eq!(without.offset(2, 1), None);
    let empty = Lines::new("");
    assert_eq!(empty.offset(1, 1), Some(0));
    assert_eq!(empty.offset(1, 2), None);
}

#[test]
fn lines_places_the_file_does_not_have_are_none() {
    let lines = Lines::new("ab\ncd\n");
    assert_eq!(lines.offset(0, 1), None);
    assert_eq!(lines.offset(1, 0), None);
    assert_eq!(lines.offset(1, 9), None);
    assert_eq!(lines.offset(9, 1), None);
}

#[test]
fn lines_span_carries_positions_and_byte_range() {
    let lines = Lines::new("xx\n\u{e9}print(1)\n");
    let span = lines.span(at(2, 2), at(2, 7));
    assert_eq!(span.as_ref().map(|s| s.bytes.clone()), Some(5..10));
    assert_eq!(span.map(|s| (s.start, s.end)), Some((at(2, 2), at(2, 7))));
}

#[test]
fn lines_span_refuses_a_reversed_or_missing_end() {
    let lines = Lines::new("abc\n");
    assert!(lines.span(at(1, 3), at(1, 1)).is_none());
    assert!(lines.span(at(1, 1), at(5, 1)).is_none());
    assert!(lines.span(at(1, 2), at(1, 2)).is_some_and(|s| s.bytes.is_empty()));
}
