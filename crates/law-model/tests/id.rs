use law_model::{Refusal, RuleId};

type Outcome = Result<(), Box<dyn std::error::Error>>;

#[test]
fn id_accepts_namespaced_words() -> Outcome {
    for text in ["lexis.elif", "ost.one-word-module", "a.b", "law2.x9-y"] {
        let id: RuleId = text.parse()?;
        assert_eq!(id.as_str(), text);
    }
    Ok(())
}

#[test]
fn id_exposes_namespace_and_name() -> Outcome {
    let id: RuleId = "ost.one-word-module".parse()?;
    assert_eq!(id.namespace(), "ost");
    assert_eq!(id.name(), "one-word-module");
    Ok(())
}

#[test]
fn id_refuses_bad_forms_as_bad_id() {
    for text in [
        "Elif",
        "lexis",
        "lexis.",
        ".elif",
        "Lexis.elif",
        "lexis.Elif",
        "lexis.a.b",
        "1x.y",
        "x.1y",
        "lexis.el if",
        "",
        "-a.b",
        "lexis.-x",
    ] {
        let refused = text.parse::<RuleId>();
        assert_eq!(
            refused.as_ref().err().map(|invalid| invalid.refusal),
            Some(Refusal::BadId),
            "{text:?} -> {refused:?}"
        );
    }
}

#[test]
fn id_deserialisation_refuses_bad_id() {
    assert!(serde_json::from_str::<RuleId>("\"Elif\"").is_err());
    assert!(serde_json::from_str::<RuleId>("\"lexis\"").is_err());
    assert!(serde_json::from_str::<RuleId>("\"lexis.\"").is_err());
    assert!(serde_json::from_str::<RuleId>("5").is_err());
}

#[test]
fn id_round_trips() -> Outcome {
    let id: RuleId = "lexis.elif".parse()?;
    let back: RuleId = serde_json::from_str(&serde_json::to_string(&id)?)?;
    assert_eq!(id, back);
    assert_eq!(serde_json::to_string(&id)?, "\"lexis.elif\"");
    Ok(())
}
