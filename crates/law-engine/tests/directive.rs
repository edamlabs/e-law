use law_engine::Directive;

fn read(reason: Option<&str>, modules: &[&str]) -> Directive {
    Directive {
        reason: reason.map(str::to_owned),
        modules: modules.iter().map(|module| (*module).to_owned()).collect(),
    }
}

#[test]
fn directive_parse_reads_tachs_grammar() {
    let table: [(&str, Option<Directive>); 30] = [
        ("# tach-ignore", Some(read(None, &[]))),
        ("#tach-ignore", Some(read(None, &[]))),
        ("#   tach-ignore", Some(read(None, &[]))),
        ("# tach-ignore ", Some(read(None, &[]))),
        ("# tach-ignore\t", Some(read(None, &[]))),
        ("#tach-ignore(reason)", Some(read(Some("reason"), &[]))),
        ("# tach-ignore()", Some(read(Some(""), &[]))),
        (
            "# tach-ignore(a reason, with words.)",
            Some(read(Some("a reason, with words."), &[])),
        ),
        ("# tach-ignore(r) a.b c.d", Some(read(Some("r"), &["a.b", "c.d"]))),
        ("# tach-ignore(r)   a.b", Some(read(Some("r"), &["a.b"]))),
        ("# tach-ignore a.b", Some(read(None, &["a.b"]))),
        ("# tach-ignore a.b c_d.e9", Some(read(None, &["a.b", "c_d.e9"]))),
        ("# tach-ignore a.b\tc", Some(read(None, &["a.b", "c"]))),
        (
            "# tach-ignore a.b trailing words",
            Some(read(None, &["a.b", "trailing", "words"])),
        ),
        ("# why # tach-ignore", Some(read(None, &[]))),
        ("# why # tach-ignore(r) a.b", Some(read(Some("r"), &["a.b"]))),
        ("# noqa # tach-ignore a.b", Some(read(None, &["a.b"]))),
        ("# tach-ignore m\u{f3}dulo.x", Some(read(None, &["m\u{f3}dulo.x"]))),
        ("# tach-ignore a.b, c", None),
        ("# tach-ignore a.b # more", None),
        ("# tach-ignore: a.b", None),
        ("# tach-ignore(r)a.b", None),
        ("# tach-ignore(r", None),
        ("# tach-ignore(a)(b)", None),
        ("# tach-ignorex", None),
        ("# tach-ignore-all", None),
        ("# note tach-ignore", None),
        ("tach-ignore", None),
        ("#\ttach-ignore", None),
        ("# TACH-IGNORE", None),
    ];
    for (comment, expected) in &table {
        assert_eq!(&Directive::parse(comment), expected, "{comment:?}");
    }
}

#[test]
fn directive_parse_first_hash_that_opens_one_wins_over_a_failing_earlier_one() {
    assert_eq!(
        Directive::parse("# tach-ignore a.b, c # tach-ignore d"),
        Some(read(None, &["d"]))
    );
}

#[test]
fn directive_parse_reason_stops_at_the_first_closing_parenthesis() {
    assert_eq!(Directive::parse("# tach-ignore(a) b"), Some(read(Some("a"), &["b"])));
    assert_eq!(Directive::parse("# tach-ignore(a)) b"), None);
}
