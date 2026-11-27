use law_model::Tags;

type Outcome = Result<(), Box<dyn std::error::Error>>;

fn tags(json: &str) -> Result<Tags, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(json)?)
}

#[test]
fn tags_integer_equals_string() -> Outcome {
    assert_eq!(tags(r#"{"law": 5}"#)?, tags(r#"{"law": "5"}"#)?);
    assert!(tags(r#"{"law": 5}"#)?.contains("law", "5"));
    Ok(())
}

#[test]
fn tags_yaml_integer_equals_yaml_string() -> Outcome {
    let integer: Tags = serde_saphyr::from_str("law: 5\n")?;
    let string: Tags = serde_saphyr::from_str("law: \"5\"\n")?;
    assert_eq!(integer, string);
    Ok(())
}

#[test]
fn tags_list_of_integers_and_strings_mix() -> Outcome {
    let tags = tags(r#"{"law": [5, "7"]}"#)?;
    assert!(tags.contains("law", "5") && tags.contains("law", "7"));
    Ok(())
}

#[test]
fn tags_scalar_and_list_load() -> Outcome {
    let tags = tags(r#"{"set": "LEXIS", "law": ["5", "7"]}"#)?;
    assert!(tags.contains("set", "LEXIS"));
    assert!(tags.contains("law", "7"));
    assert!(!tags.contains("law", "6"));
    assert!(!tags.contains("missing", "x"));
    Ok(())
}

#[test]
fn tags_serialise_scalar_when_one_list_when_many() -> Outcome {
    let json = serde_json::to_value(tags(r#"{"a": ["x"], "b": ["x", "y"], "c": 5}"#)?)?;
    assert_eq!(json, serde_json::json!({"a": "x", "b": ["x", "y"], "c": "5"}));
    Ok(())
}

#[test]
fn tags_round_trip_scalar_integer_and_list() -> Outcome {
    for source in [
        r#"{"set": "LEXIS"}"#,
        r#"{"law": 5}"#,
        r#"{"law": ["5", "7"], "set": ["OST", "LEXIS"]}"#,
        r#"{"law": [5]}"#,
        "{}",
    ] {
        let tags = tags(source)?;
        let back: Tags = serde_json::from_str(&serde_json::to_string(&tags)?)?;
        assert_eq!(back, tags, "{source}");
    }
    Ok(())
}

#[test]
fn tags_round_trip_through_yaml() -> Outcome {
    let tags: Tags = serde_saphyr::from_str("law: [5, '7']\nset: LEXIS\n")?;
    let back: Tags = serde_saphyr::from_str(&serde_saphyr::to_string(&tags)?)?;
    assert_eq!(back, tags);
    Ok(())
}

#[test]
fn tags_list_dedups_and_orders() -> Outcome {
    let json = serde_json::to_value(tags(r#"{"k": ["b", "a", "b"]}"#)?)?;
    assert_eq!(json, serde_json::json!({"k": ["a", "b"]}));
    Ok(())
}

#[test]
fn tags_refuse_nested_values() {
    assert!(serde_json::from_str::<Tags>(r#"{"k": {"a": 1}}"#).is_err());
    assert!(serde_json::from_str::<Tags>(r#"{"k": [[1]]}"#).is_err());
    assert!(serde_json::from_str::<Tags>(r#"{"k": null}"#).is_err());
}

#[test]
fn tags_iter_yields_keys_and_values_in_order() -> Outcome {
    let tags = tags(r#"{"b": ["y", "x"], "a": "1"}"#)?;
    let seen: Vec<(String, Vec<String>)> = tags
        .iter()
        .map(|(key, values)| (key.to_owned(), values.map(str::to_owned).collect()))
        .collect();
    assert_eq!(
        seen,
        vec![
            ("a".to_owned(), vec!["1".to_owned()]),
            ("b".to_owned(), vec!["x".to_owned(), "y".to_owned()]),
        ]
    );
    Ok(())
}
