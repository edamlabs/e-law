use law_config::schema;
use law_model::{Case, Rule, Snippet};
use schemars::Schema;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

type Outcome = Result<(), Box<dyn std::error::Error>>;

fn accepts(schema: &Schema, instance: &Value) -> Result<bool, Box<dyn std::error::Error>> {
    let validator = jsonschema::draft202012::options()
        .should_validate_formats(true)
        .build(&serde_json::to_value(schema)?)?;
    Ok(validator.is_valid(instance))
}

fn rule(check: &Value, cases: &Value) -> Value {
    json!([{"id": "a.b", "message": "m", "check": check, "fix": {"safety": "none", "reason": "r"}, "cases": cases}])
}

fn graph(extra: Value) -> Value {
    let mut check = json!({"engine": "graph", "aspect": "cycle"});
    if let (Some(map), Value::Object(more)) = (check.as_object_mut(), extra) {
        map.extend(more);
    }
    rule(&check, &json!({}))
}

fn disagreements<T: DeserializeOwned>(cases: &[(&str, Value)]) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    cases
        .iter()
        .map(|(name, instance)| {
            let serde_accepts = serde_json::from_value::<T>(instance.clone()).is_ok();
            let schema_accepts = accepts(&schema().rules, instance)?;
            Ok((serde_accepts, schema_accepts, name))
        })
        .filter_map(|result: Result<_, Box<dyn std::error::Error>>| match result {
            Ok((serde_accepts, schema_accepts, _)) if serde_accepts == schema_accepts => None,
            Ok((serde_accepts, schema_accepts, name)) => Some(Ok(format!(
                "{name}: serde accepts={serde_accepts}, schema accepts={schema_accepts}"
            ))),
            Err(error) => Some(Err(error)),
        })
        .collect()
}

#[test]
fn graph_spec_schema_and_serde_agree_on_each_check() -> Outcome {
    let mismatches = disagreements::<Vec<Rule>>(&[
        ("cycle", graph(json!({}))),
        ("orphan", graph(json!({"aspect": "orphan"}))),
        (
            "orphan with entries",
            graph(json!({"aspect": "orphan", "entries": ["src/*/main.py", "tests/**"]})),
        ),
        ("typing true", graph(json!({"typing": true}))),
        ("typing false", graph(json!({"typing": false}))),
        ("no aspect", rule(&json!({"engine": "graph"}), &json!({}))),
        ("unknown aspect", graph(json!({"aspect": "layers"}))),
        ("aspect in capitals", graph(json!({"aspect": "Cycle"}))),
        ("unknown key", graph(json!({"depth": 2}))),
        ("typing not a bool", graph(json!({"typing": "yes"}))),
        ("entries not a list", graph(json!({"entries": "src/*"}))),
        ("entries not globs", graph(json!({"entries": [3]}))),
    ])?;
    assert!(mismatches.is_empty(), "{mismatches:#?}");
    Ok(())
}

#[test]
fn graph_case_schema_and_serde_agree_on_every_shape() -> Outcome {
    let check = json!({"engine": "graph", "aspect": "cycle"});
    let on = |cases: Value| rule(&check, &cases);
    let mismatches = disagreements::<Vec<Rule>>(&[
        ("bare string", on(json!({"valid": ["x = 1\n"]}))),
        ("code", on(json!({"valid": [{"code": "x = 1\n"}]}))),
        (
            "code and path",
            on(json!({"valid": [{"code": "x", "path": "src/a.py"}]})),
        ),
        ("files", on(json!({"valid": [{"files": {"src/a.py": "x = 1\n"}}]}))),
        (
            "files of two",
            on(json!({"invalid": [{"files": {"src/a.py": "import b\n", "src/b.py": "import a\n"}}]})),
        ),
        (
            "code and files",
            on(json!({"valid": [{"code": "x", "files": {"a.py": "x"}}]})),
        ),
        (
            "files and path",
            on(json!({"valid": [{"path": "a.py", "files": {"a.py": "x"}}]})),
        ),
        ("neither", on(json!({"valid": [{}]}))),
        ("path only", on(json!({"valid": [{"path": "a.py"}]}))),
        ("empty files", on(json!({"valid": [{"files": {}}]}))),
        ("files null", on(json!({"valid": [{"files": null}]}))),
        (
            "files and null code",
            on(json!({"valid": [{"files": {"a.py": "x"}, "code": null}]})),
        ),
        ("code null and files absent", on(json!({"valid": [{"code": null}]}))),
        ("files of a list", on(json!({"valid": [{"files": ["a.py"]}]}))),
        (
            "files value not a string",
            on(json!({"valid": [{"files": {"a.py": 1}}]})),
        ),
        (
            "unknown key",
            on(json!({"valid": [{"files": {"a.py": "x"}, "extra": 1}]})),
        ),
        (
            "fixed with files",
            on(json!({"fixed": [{"files": {"a.py": "x"}, "expect": "y"}]})),
        ),
    ])?;
    assert!(mismatches.is_empty(), "{mismatches:#?}");
    Ok(())
}

#[test]
fn graph_case_serde_reads_each_shape_into_its_snippet() -> Outcome {
    let code: Case = serde_json::from_value(json!({"path": "src/a.py", "code": "x = 1\n"}))?;
    assert_eq!(code.snippet, Snippet::Code("x = 1\n".to_owned()));
    assert_eq!(code.path.as_deref(), Some(std::path::Path::new("src/a.py")));
    let bare: Case = serde_json::from_value(json!("y = 2\n"))?;
    assert_eq!((bare.path, bare.snippet), (None, Snippet::Code("y = 2\n".to_owned())));
    let files: Case = serde_json::from_value(json!({"files": {"src/b.py": "b\n", "src/a.py": "a\n"}}))?;
    assert!(files.path.is_none());
    let Snippet::Files(map) = files.snippet else {
        return Err("not files".into());
    };
    assert_eq!(
        map.keys()
            .map(|path| path.to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        ["src/a.py", "src/b.py"]
    );
    Ok(())
}

#[test]
fn graph_case_round_trips_through_json_and_yaml() -> Outcome {
    for case in [
        json!({"path": "src/a.py", "code": "x = 1\n"}),
        json!({"code": "x = 1\n"}),
        json!({"files": {"src/a.py": "import b\n", "src/b.py": "import a\n"}}),
    ] {
        let parsed: Case = serde_json::from_value(case.clone())?;
        let back: Case = serde_json::from_str(&serde_json::to_string(&parsed)?)?;
        assert_eq!(back, parsed, "{case}");
        let yaml: Case = serde_saphyr::from_str(&serde_saphyr::to_string(&parsed)?)?;
        assert_eq!(yaml, parsed, "{case}");
        assert_eq!(serde_json::to_value(&parsed)?, case);
    }
    Ok(())
}

#[test]
fn graph_spec_serialises_without_its_defaults() -> Outcome {
    let rules: Vec<Rule> = serde_json::from_value(graph(json!({})))?;
    let back = serde_json::to_value(&rules)?;
    assert_eq!(back[0]["check"], json!({"engine": "graph", "aspect": "cycle"}));
    let full: Vec<Rule> = serde_json::from_value(graph(json!({"typing": true, "entries": ["a/**"]})))?;
    assert_eq!(
        serde_json::to_value(&full)?[0]["check"],
        json!({"engine": "graph", "aspect": "cycle", "entries": ["a/**"], "typing": true})
    );
    Ok(())
}

#[test]
fn graph_spec_serde_refuses_an_entry_that_is_no_glob() {
    assert!(serde_json::from_value::<Vec<Rule>>(graph(json!({"entries": ["a/[x"]}))).is_err());
}
