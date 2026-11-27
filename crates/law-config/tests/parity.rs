use law_config::schema;
use law_model::{Entries, Of, Relation, Rule};
use serde_json::{Value, json};

type Outcome = Result<(), Box<dyn std::error::Error>>;

fn accepts(instance: &Value) -> Result<bool, Box<dyn std::error::Error>> {
    let validator = jsonschema::draft202012::options()
        .should_validate_formats(true)
        .build(&serde_json::to_value(schema().rules)?)?;
    Ok(validator.is_valid(instance))
}

fn rule(check: &Value) -> Value {
    json!([{"id": "a.b", "message": "m", "check": check, "fix": {"safety": "none", "reason": "r"}}])
}

fn parity((names, entries, relation): &(Value, Value, Value)) -> Value {
    rule(&json!({"engine": "parity", "names": names, "entries": entries, "relation": relation}))
}

fn good() -> (Value, Value, Value) {
    (
        json!({"class": "{Dir}State", "of": "attributes"}),
        json!({"beside": true}),
        json!("equal"),
    )
}

fn with(replace: impl FnOnce(&mut (Value, Value, Value))) -> Value {
    let mut parts = good();
    replace(&mut parts);
    parity(&parts)
}

#[test]
fn parity_spec_schema_and_serde_agree_on_each_check() -> Outcome {
    let cases: Vec<(&str, Value)> = vec![
        ("beside", with(|_| {})),
        ("members", with(|p| p.0 = json!({"class": "Kind", "of": "members"}))),
        ("glob", with(|p| p.1 = json!({"glob": "src/*/agents/*.md"}))),
        (
            "beside false and glob",
            with(|p| p.1 = json!({"beside": false, "glob": "src/*.md"})),
        ),
        (
            "beside with exclude",
            with(|p| p.1 = json!({"beside": true, "exclude": ["state", "ports"]})),
        ),
        (
            "glob with exclude",
            with(|p| p.1 = json!({"glob": "a/*.md", "exclude": ["x"]})),
        ),
        ("names-in-entries", with(|p| p.2 = json!("names-in-entries"))),
        ("entries-in-names", with(|p| p.2 = json!("entries-in-names"))),
        ("neither source", with(|p| p.1 = json!({"exclude": ["state"]}))),
        ("empty entries", with(|p| p.1 = json!({}))),
        ("beside false alone", with(|p| p.1 = json!({"beside": false}))),
        (
            "beside and glob",
            with(|p| p.1 = json!({"beside": true, "glob": "a/*.md"})),
        ),
        ("glob null", with(|p| p.1 = json!({"glob": null}))),
        ("beside not a bool", with(|p| p.1 = json!({"beside": "yes"}))),
        (
            "exclude not a list",
            with(|p| p.1 = json!({"beside": true, "exclude": "state"})),
        ),
        (
            "exclude not strings",
            with(|p| p.1 = json!({"beside": true, "exclude": [1]})),
        ),
        ("entries key", with(|p| p.1 = json!({"beside": true, "depth": 2}))),
        (
            "names key",
            with(|p| p.0 = json!({"class": "X", "of": "members", "depth": 2})),
        ),
        ("no class", with(|p| p.0 = json!({"of": "members"}))),
        ("no of", with(|p| p.0 = json!({"class": "X"}))),
        ("unknown of", with(|p| p.0 = json!({"class": "X", "of": "fields"}))),
        ("of in capitals", with(|p| p.0 = json!({"class": "X", "of": "Members"}))),
        (
            "class not a string",
            with(|p| p.0 = json!({"class": 1, "of": "members"})),
        ),
        ("unknown relation", with(|p| p.2 = json!("same"))),
        ("relation in capitals", with(|p| p.2 = json!("Equal"))),
        (
            "no relation",
            rule(&json!({"engine": "parity", "names": good().0, "entries": good().1})),
        ),
        (
            "no names",
            rule(&json!({"engine": "parity", "entries": good().1, "relation": "equal"})),
        ),
        (
            "no entries",
            rule(&json!({"engine": "parity", "names": good().0, "relation": "equal"})),
        ),
        (
            "unknown key",
            rule(&json!({"engine": "parity", "names": good().0, "entries": good().1, "relation": "equal", "depth": 1})),
        ),
    ];
    let mismatches: Vec<String> = cases
        .iter()
        .map(|(name, instance)| {
            let serde_accepts = serde_json::from_value::<Vec<Rule>>(instance.clone()).is_ok();
            accepts(instance).map(|schema_accepts| (serde_accepts, schema_accepts, name))
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|(serde_accepts, schema_accepts, _)| serde_accepts != schema_accepts)
        .map(|(serde_accepts, schema_accepts, name)| {
            format!("{name}: serde accepts={serde_accepts}, schema accepts={schema_accepts}")
        })
        .collect();
    assert!(mismatches.is_empty(), "{mismatches:#?}");
    Ok(())
}

#[test]
fn parity_spec_serialises_without_its_defaults() -> Outcome {
    let rules: Vec<Rule> = serde_json::from_value(with(|_| {}))?;
    assert_eq!(
        serde_json::to_value(&rules)?[0]["check"],
        json!({
            "engine": "parity",
            "names": {"class": "{Dir}State", "of": "attributes"},
            "entries": {"beside": true},
            "relation": "equal",
        })
    );
    let full: Vec<Rule> = serde_json::from_value(with(|p| {
        p.0 = json!({"class": "Kind", "of": "members"});
        p.1 = json!({"glob": "a/*.md", "exclude": ["x"]});
        p.2 = json!("entries-in-names");
    }))?;
    assert_eq!(
        serde_json::to_value(&full)?[0]["check"],
        json!({
            "engine": "parity",
            "names": {"class": "Kind", "of": "members"},
            "entries": {"glob": "a/*.md", "exclude": ["x"]},
            "relation": "entries-in-names",
        })
    );
    Ok(())
}

#[test]
fn parity_entries_refuse_neither_and_both_sources_with_their_code() {
    for entries in [
        json!({}),
        json!({"beside": false}),
        json!({"beside": true, "glob": "a/*"}),
    ] {
        let error = serde_json::from_value::<Entries>(entries.clone())
            .err()
            .map(|error| error.to_string());
        assert!(error.is_some_and(|text| text.contains("bad-entries")), "{entries}");
    }
}

#[test]
fn parity_words_are_written_in_kebab_case() -> Outcome {
    assert_eq!(serde_json::to_value(Of::Members)?, json!("members"));
    assert_eq!(
        serde_json::to_value(Relation::NamesInEntries)?,
        json!("names-in-entries")
    );
    assert_eq!(Relation::EntriesInNames.to_string(), "entries-in-names");
    Ok(())
}

#[test]
fn parity_spec_serde_refuses_a_glob_that_does_not_compile() {
    assert!(serde_json::from_value::<Vec<Rule>>(with(|p| p.1 = json!({"glob": "a/[x"}))).is_err());
}
