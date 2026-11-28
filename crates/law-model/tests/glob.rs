use law_model::{Expression, Glob, Kind, Refusal, Template};

type Outcome = Result<(), Box<dyn std::error::Error>>;

#[test]
fn glob_star_does_not_cross_slash() -> Outcome {
    let glob: Glob = "src/*.py".parse()?;
    assert!(glob.matches("src/a.py"));
    assert!(!glob.matches("src/a/b.py"));
    assert!(!glob.matches("src/a/b/c.py"));
    Ok(())
}

#[test]
fn glob_double_star_crosses_slash() -> Outcome {
    let glob: Glob = "src/**/*.py".parse()?;
    assert!(glob.matches("src/a.py"));
    assert!(glob.matches("src/a/b.py"));
    assert!(glob.matches("src/a/b/c.py"));
    assert!(!glob.matches("lib/a.py"));
    Ok(())
}

#[test]
fn glob_leading_double_star_matches_any_depth() -> Outcome {
    let glob: Glob = "**/*.py".parse()?;
    assert!(glob.matches("a.py"));
    assert!(glob.matches("a/b/c.py"));
    assert!(!glob.matches("a/b/c.rs"));
    Ok(())
}

#[test]
fn glob_refuses_unclosed_class_as_bad_glob() {
    for text in ["src/[ab", "src/{a,b"] {
        assert_eq!(
            text.parse::<Glob>().err().map(|invalid| invalid.refusal),
            Some(Refusal::BadGlob),
            "{text:?}"
        );
    }
}

#[test]
fn glob_round_trips_as_text() -> Outcome {
    let glob: Glob = "src/{a,b}/*.py".parse()?;
    let json = serde_json::to_string(&glob)?;
    assert_eq!(json, "\"src/{a,b}/*.py\"");
    assert_eq!(serde_json::from_str::<Glob>(&json)?, glob);
    Ok(())
}

#[test]
fn glob_deserialisation_refuses_bad_glob() {
    assert!(serde_json::from_str::<Glob>("\"src/[ab\"").is_err());
}

#[test]
fn expression_matches_whole_text_only() -> Outcome {
    let expression: Expression = "[a-z]+".parse()?;
    assert!(expression.matches("abc"));
    assert!(!expression.matches("abc1"));
    assert!(!expression.matches("1abc"));
    Ok(())
}

#[test]
fn expression_refuses_bad_regex() {
    assert_eq!(
        "(".parse::<Expression>().err().map(|invalid| invalid.refusal),
        Some(Refusal::BadExpression)
    );
}

#[test]
fn kind_refuses_non_word() {
    for text in ["Engine", "two words", "", "a.b", "1a"] {
        assert_eq!(
            text.parse::<Kind>().err().map(|invalid| invalid.refusal),
            Some(Refusal::BadKind),
            "{text:?}"
        );
    }
}

#[test]
fn template_fills_dir_and_stem() -> Outcome {
    let template: Template = "{Stem}{Dir}".parse()?;
    let glob = template.fill("Engine", "Cache")?;
    assert!(glob.matches("CacheEngine"));
    Ok(())
}

#[test]
fn template_refuses_broken_glob_as_bad_template() {
    assert_eq!(
        "[{Dir}".parse::<Template>().err().map(|invalid| invalid.refusal),
        Some(Refusal::BadTemplate)
    );
}

#[test]
fn template_refuses_an_unknown_placeholder_and_an_empty_text() {
    for text in ["{Nope}State", ""] {
        assert_eq!(
            text.parse::<Template>().err().map(|invalid| invalid.refusal),
            Some(Refusal::BadTemplate),
            "{text}"
        );
    }
    assert!("{Dir}{A,B}*".parse::<Template>().is_ok());
}
