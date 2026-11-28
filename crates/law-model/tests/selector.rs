use law_model::{Refusal, Rule, Selector};
use serde_json::json;

type Outcome = Result<(), Box<dyn std::error::Error>>;

fn rule(id: &str, tags: serde_json::Value) -> Result<Rule, Box<dyn std::error::Error>> {
    let mut value = json!({
        "id": id,
        "message": "m",
        "check": {"engine": "pattern", "rule": {"kind": "x"}},
        "fix": {"safety": "safe"},
    });
    value["tags"] = tags;
    Ok(serde_json::from_value(value)?)
}

fn selects(selector: &str, rule: &Rule) -> Result<bool, Box<dyn std::error::Error>> {
    Ok(selector.parse::<Selector>()?.matches(rule))
}

#[test]
fn selector_tag_matches_string_integer_and_list() -> Outcome {
    for tags in [
        json!({"law": "5"}),
        json!({"law": 5}),
        json!({"law": ["5", "7"]}),
        json!({"law": [7, 5]}),
    ] {
        assert!(selects("law=5", &rule("lexis.elif", tags.clone())?)?, "{tags}");
    }
    Ok(())
}

#[test]
fn selector_tag_does_not_match_other_value_or_key() -> Outcome {
    assert!(!selects("law=5", &rule("lexis.elif", json!({"law": "6"}))?)?);
    assert!(!selects("law=5", &rule("lexis.elif", json!({"set": "5"}))?)?);
    assert!(!selects("law=5", &rule("lexis.elif", json!({}))?)?);
    assert!(!selects("law=5", &rule("lexis.elif", json!({"law": "55"}))?)?);
    Ok(())
}

#[test]
fn selector_glob_matches_namespace_only() -> Outcome {
    assert!(selects("lexis.*", &rule("lexis.elif", json!({}))?)?);
    assert!(selects("lexis.*", &rule("lexis.one-word", json!({}))?)?);
    assert!(!selects("lexis.*", &rule("lexis2.x", json!({}))?)?);
    assert!(!selects("lexis.*", &rule("ost.elif", json!({}))?)?);
    Ok(())
}

#[test]
fn selector_id_matches_exactly() -> Outcome {
    assert!(selects("lexis.elif", &rule("lexis.elif", json!({}))?)?);
    assert!(!selects("lexis.elif", &rule("lexis.elif2", json!({}))?)?);
    assert!(!selects("lexis.elif", &rule("lexis.el", json!({}))?)?);
    Ok(())
}

#[test]
fn selector_all_matches_everything() -> Outcome {
    assert!(selects("ALL", &rule("a.b", json!({}))?)?);
    assert!(selects("ALL", &rule("z.y", json!({"law": 1}))?)?);
    Ok(())
}

#[test]
fn selector_parses_each_form_to_its_variant() -> Outcome {
    assert_eq!("ALL".parse::<Selector>()?, Selector::All);
    assert!(matches!("lexis.elif".parse::<Selector>()?, Selector::Id(_)));
    assert!(matches!("lexis.*".parse::<Selector>()?, Selector::Glob(_)));
    assert!(matches!("law=5".parse::<Selector>()?, Selector::Tag { .. }));
    Ok(())
}

#[test]
fn selector_refuses_bad_forms_as_bad_selector() {
    for text in [
        "",
        "=5",
        "law=",
        "a=b=c",
        "Lexis.elif",
        "lexis",
        "all",
        "lexis.[",
        "lexis elif",
    ] {
        let parsed = text.parse::<Selector>();
        assert_eq!(
            parsed.as_ref().err().map(|invalid| invalid.refusal),
            Some(Refusal::BadSelector),
            "{text:?} -> {parsed:?}"
        );
    }
}

#[test]
fn selector_deserialisation_refuses_bad_forms() {
    assert!(serde_json::from_str::<Selector>("\"=5\"").is_err());
    assert!(serde_json::from_str::<Selector>("\"lexis\"").is_err());
}

#[test]
fn selector_round_trips() -> Outcome {
    for text in ["ALL", "lexis.elif", "lexis.*", "law=5"] {
        let selector: Selector = text.parse()?;
        assert_eq!(selector.to_string(), text);
        let back: Selector = serde_json::from_str(&serde_json::to_string(&selector)?)?;
        assert_eq!(back, selector, "{text}");
    }
    Ok(())
}

#[test]
fn selector_glob_with_uppercase_is_refused() {
    let parsed = "Lexis.*".parse::<Selector>();
    assert_eq!(
        parsed.as_ref().err().map(|invalid| invalid.refusal),
        Some(Refusal::BadSelector),
        "an id glob can never match a rule id with a capital: {parsed:?}"
    );
}
