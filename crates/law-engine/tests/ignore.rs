use law_engine::Ignore;

fn names(list: &[&str]) -> Ignore {
    Ignore::Names(list.iter().map(|name| (*name).to_owned()).collect())
}

#[test]
fn ignore_parse_reads_tys_grammar() {
    let table: [(&str, Vec<Ignore>); 19] = [
        ("# ty: ignore", vec![Ignore::All]),
        ("# ty:ignore", vec![Ignore::All]),
        ("#ty: ignore", vec![Ignore::All]),
        ("# type: ignore", vec![Ignore::All]),
        ("#type:ignore", vec![Ignore::All]),
        ("# ty: ignore[a]", vec![names(&["a"])]),
        ("# ty: ignore[a, b]", vec![names(&["a", "b"])]),
        ("# ty: ignore[ a , b ]", vec![names(&["a", "b"])]),
        ("# ty: ignore [a]", vec![names(&["a"])]),
        ("# ty: ignore[invalid-assignment]", vec![names(&["invalid-assignment"])]),
        ("# ty: ignore  # because", vec![Ignore::All]),
        ("# type: ignore[ty:x]", vec![names(&["x"])]),
        ("# type: ignore[ty:x, attr-defined]", vec![names(&["x"])]),
        ("# type: ignore[attr-defined]", vec![]),
        ("# ty: ignore[]", vec![]),
        ("# type: ignore[]", vec![]),
        ("# ty: ignore[a", vec![]),
        ("# ty: ignored", vec![]),
        ("# ty: ignoreall", vec![]),
    ];
    for (comment, expected) in &table {
        assert_eq!(&Ignore::parse(comment), expected, "{comment:?}");
    }
}

#[test]
fn ignore_parse_a_comment_that_says_nothing_of_ignore_is_none() {
    for comment in ["", "# plain", "# ty: check", "# type: str", "# noqa", "# ignore"] {
        assert!(Ignore::parse(comment).is_empty(), "{comment:?}");
    }
}

#[test]
fn ignore_parse_one_comment_may_open_several_suppressions() {
    assert_eq!(
        Ignore::parse("# ty: ignore[a]  # type: ignore[ty:b]"),
        vec![names(&["a"]), names(&["b"])]
    );
}
