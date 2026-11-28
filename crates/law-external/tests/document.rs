mod common;

use common::{Outcome, fixture};
use law_external::{Applicability, RuffDiagnostic, Sarif, SarifArtifact};
use serde_json::{Value, json};
use std::path::PathBuf;

fn ruff(name: &str) -> Result<Vec<RuffDiagnostic>, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(&fixture(name)?)?)
}

fn grown(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.values_mut().for_each(grown);
            map.insert("added_by_a_newer_tool".to_owned(), json!({"x": [1, 2]}));
        }
        Value::Array(items) => items.iter_mut().for_each(grown),
        _ => {}
    }
}

#[test]
fn document_ruff_sample_parses_every_field_e_law_reads() -> Outcome {
    let diagnostics = ruff("ruff.json")?;
    let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["F401", "UP045", "T201", "B904"]);
    let print = &diagnostics[2];
    assert_eq!((print.location.row, print.location.column), (5, 17));
    assert_eq!((print.end_location.row, print.end_location.column), (5, 22));
    assert_eq!(print.message, "`print` found");
    assert_eq!(print.filename, "/tmp/lawobs/a.py");
    let fix = print.fix.as_ref().ok_or("T201 has a fix")?;
    assert_eq!(fix.applicability, Applicability::Unsafe);
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].content, "");
    assert_eq!(fix.edits[0].end_location.column, 25);
    assert_eq!(
        diagnostics[0].fix.as_ref().map(|f| f.applicability),
        Some(Applicability::Safe)
    );
    assert!(diagnostics[3].fix.is_none());
    Ok(())
}

#[test]
fn document_ruff_sample_with_unknown_extra_fields_still_parses() -> Outcome {
    let mut value: Value = serde_json::from_str(&fixture("ruff.json")?)?;
    grown(&mut value);
    let grown: Vec<RuffDiagnostic> = serde_json::from_value(value)?;
    assert_eq!(grown, ruff("ruff.json")?);
    Ok(())
}

#[test]
fn document_ruff_syntax_error_is_a_diagnostic_with_a_name_for_a_code() -> Outcome {
    let diagnostics = ruff("ruff-syntax.json")?;
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "invalid-syntax");
    assert!(diagnostics[0].fix.is_none());
    Ok(())
}

#[test]
fn document_ruff_empty_array_is_no_diagnostics() -> Outcome {
    assert!(serde_json::from_str::<Vec<RuffDiagnostic>>("[]")?.is_empty());
    Ok(())
}

#[test]
fn document_ruff_refuses_what_is_not_the_document() {
    let wrong = [
        "",
        "not json",
        "{}",
        "[{\"code\": \"T201\"}]",
        "[{\"code\":\"T201\",\"filename\":\"a\",\"location\":{\"row\":1,\"column\":1},\"end_location\":{\"row\":1,\"column\":2},\"message\":\"m\",\"fix\":{\"applicability\":\"weird\",\"edits\":[]}}]",
    ];
    for text in wrong {
        assert!(serde_json::from_str::<Vec<RuffDiagnostic>>(text).is_err(), "{text}");
    }
}

#[test]
fn document_ty_sample_parses_the_result_its_place_and_its_file() -> Outcome {
    let sarif: Sarif = serde_json::from_str(&fixture("ty.sarif")?)?;
    assert_eq!(sarif.runs.len(), 1);
    let result = &sarif.runs[0].results[0];
    assert_eq!(result.rule_id, "invalid-explicit-return");
    assert_eq!(
        result.message.text,
        "Return type does not match returned value: expected `str`, found `int`"
    );
    let physical = &result.locations[0].physical_location;
    let region = physical.region;
    assert_eq!(
        (
            region.start_line,
            region.start_column,
            region.end_line,
            region.end_column
        ),
        (2, 24, 2, 25)
    );
    assert_eq!(
        physical.artifact_location.path(),
        Some(PathBuf::from("/tmp/lawobs/typ/m.py"))
    );
    Ok(())
}

#[test]
fn document_ty_sample_with_unknown_extra_fields_still_parses() -> Outcome {
    let plain: Sarif = serde_json::from_str(&fixture("ty.sarif")?)?;
    let mut value: Value = serde_json::from_str(&fixture("ty.sarif")?)?;
    grown(&mut value);
    assert_eq!(serde_json::from_value::<Sarif>(value)?, plain);
    Ok(())
}

#[test]
fn document_ty_result_without_locations_parses_as_placeless() -> Outcome {
    let text = r#"{"runs":[{"results":[{"ruleId":"r","message":{"text":"m"}}]}]}"#;
    let sarif: Sarif = serde_json::from_str(text)?;
    assert!(sarif.runs[0].results[0].locations.is_empty());
    Ok(())
}

#[test]
fn document_ty_no_results_is_an_empty_run() -> Outcome {
    let sarif: Sarif = serde_json::from_str(r#"{"runs":[{"results":[]}]}"#)?;
    assert!(sarif.runs[0].results.is_empty());
    Ok(())
}

#[test]
fn document_ty_uri_decodes_percent_escapes_to_utf8() {
    let path = |uri: &str| SarifArtifact { uri: uri.to_owned() }.path();
    assert_eq!(
        path("file:///tmp/a%20b/%C3%A9.py"),
        Some(PathBuf::from("/tmp/a b/\u{e9}.py"))
    );
    assert_eq!(path("file:///plain/x.py"), Some(PathBuf::from("/plain/x.py")));
    assert_eq!(path("file:///tmp/100%25.py"), Some(PathBuf::from("/tmp/100%.py")));
}

#[test]
fn document_ty_uri_refused_when_not_a_file_uri_or_not_decodable() {
    let path = |uri: &str| SarifArtifact { uri: uri.to_owned() }.path();
    assert_eq!(path("https://x/a.py"), None);
    assert_eq!(path("/tmp/a.py"), None);
    assert_eq!(path("file:///tmp/a%4"), None);
    assert_eq!(path("file:///tmp/a%zz"), None);
    assert_eq!(path("file:///tmp/a%FF.py"), None);
}
