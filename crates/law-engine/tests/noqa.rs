use law_engine::Noqa;

fn codes(list: &[&str]) -> Noqa {
    Noqa::Codes(list.iter().map(|code| (*code).to_owned()).collect())
}

#[test]
fn noqa_parse_reads_ruffs_grammar() {
    let table: [(&str, Vec<Noqa>); 24] = [
        ("# noqa", vec![Noqa::All]),
        ("#noqa", vec![Noqa::All]),
        ("# NOQA", vec![Noqa::All]),
        ("# noqa:E501", vec![codes(&["E501"])]),
        ("#noqa:E501", vec![codes(&["E501"])]),
        ("# noqa: E501", vec![codes(&["E501"])]),
        ("# NOQA : E501,W291", vec![codes(&["E501", "W291"])]),
        ("# noqa: E501, W291", vec![codes(&["E501", "W291"])]),
        ("# noqa: E501 W291", vec![codes(&["E501", "W291"])]),
        ("# noqa:E501,", vec![codes(&["E501"])]),
        ("# noqa: E501 # reason", vec![codes(&["E501"])]),
        ("# noqa: E501, W291 trailing words", vec![codes(&["E501", "W291"])]),
        ("# words # noqa: E501", vec![codes(&["E501"])]),
        ("# noqa F401", vec![Noqa::All]),
        ("# noqaE501", vec![]),
        ("# noqa:", vec![]),
        ("# noqa: ", vec![]),
        ("# noqa: e501", vec![]),
        ("# noqa: E501abc", vec![]),
        ("# noqa: E", vec![]),
        ("# ruff: noqa", vec![Noqa::All]),
        ("# ruff: noqa: E501", vec![codes(&["E501"])]),
        ("# flake8: noqa", vec![Noqa::All]),
        ("# flake8: noqa: E501", vec![codes(&["E501"])]),
    ];
    for (comment, expected) in &table {
        assert_eq!(&Noqa::parse(comment), expected, "{comment:?}");
    }
}

#[test]
fn noqa_parse_a_comment_that_says_nothing_of_noqa_is_none() {
    for comment in [
        "",
        "# just a comment",
        "# no qa",
        "x = 1",
        "# ruff: ignore",
        "# ruffnoqa",
        "# mypy: noqa",
    ] {
        assert!(Noqa::parse(comment).is_empty(), "{comment:?}");
    }
}

#[test]
fn noqa_parse_one_comment_holds_its_inline_and_its_file_level_directive() {
    assert_eq!(
        Noqa::parse("# noqa: E501 # ruff: noqa: F401"),
        vec![codes(&["E501"]), codes(&["F401"])]
    );
}

#[test]
fn noqa_parse_a_malformed_directive_is_none_not_a_blanket() {
    assert!(!Noqa::parse("# noqa: lowercase words").contains(&Noqa::All));
}
