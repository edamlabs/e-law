use std::collections::BTreeMap;

use law_model::{Catalogue, Finding, Profile, Report, Run, Severity, Span, Summary};
use serde_json::{Value, json};

type Outcome = Result<(), Box<dyn std::error::Error>>;

fn run() -> Result<Run, Box<dyn std::error::Error>> {
    Ok(serde_json::from_value(json!({
        "tool": "0.1.0",
        "commit": "abc123",
        "date": "2026-11-27T10:00:00Z",
        "catalogues": [{"name": "eager", "version": "1", "digest": "sha256:00"}],
        "select": ["ALL"],
        "ignore": ["law=prompts", "lexis.loop"],
        "paths": ["src", "tests"],
    }))?)
}

fn finding(
    rule: &str,
    path: &str,
    start: usize,
    severity: &str,
    tags: Value,
) -> Result<Finding, Box<dyn std::error::Error>> {
    let mut value = json!({
        "rule": rule,
        "path": path,
        "span": {"start": {"line": 1, "column": 1}, "end": {"line": 1, "column": 2}, "bytes": {"start": start, "end": start + 1}},
        "message": "m",
        "severity": severity,
    });
    value["tags"] = tags;
    Ok(serde_json::from_value(value)?)
}

fn findings() -> Result<Vec<Finding>, Box<dyn std::error::Error>> {
    Ok(vec![
        finding(
            "lexis.b",
            "src/z.py",
            4,
            "warning",
            json!({"law": ["5", "7"], "set": "LEXIS"}),
        )?,
        finding("lexis.a", "src/a.py", 9, "error", json!({"law": "5"}))?,
        finding("lexis.a", "src/a.py", 1, "error", json!({"law": 5, "set": "LEXIS"}))?,
        finding("ost.c", "src/m.py", 0, "error", json!({}))?,
    ])
}

#[test]
fn report_new_sorts_findings() -> Outcome {
    let report = Report::new(run()?, findings()?);
    let order: Vec<(String, usize)> = report
        .findings
        .iter()
        .map(|f| (f.path.display().to_string(), f.span.bytes.start))
        .collect();
    assert_eq!(
        order,
        vec![
            ("src/a.py".to_owned(), 1),
            ("src/a.py".to_owned(), 9),
            ("src/m.py".to_owned(), 0),
            ("src/z.py".to_owned(), 4),
        ]
    );
    assert!(report.findings.is_sorted());
    Ok(())
}

#[test]
fn report_new_is_independent_of_input_order() -> Outcome {
    let forward = Report::new(run()?, findings()?);
    let mut reversed = findings()?;
    reversed.reverse();
    assert_eq!(Report::new(run()?, reversed), forward);
    Ok(())
}

#[test]
fn summary_counts_agree_with_findings() -> Outcome {
    let report = Report::new(run()?, findings()?);
    let summary = &report.summary;
    assert_eq!(summary.total, report.findings.len());
    assert_eq!(summary.by_severity.values().sum::<usize>(), summary.total);
    assert_eq!(summary.by_rule.values().sum::<usize>(), summary.total);
    assert_eq!(summary.by_severity.get(&Severity::Error), Some(&3));
    assert_eq!(summary.by_severity.get(&Severity::Warning), Some(&1));
    assert_eq!(summary.by_rule.get(&"lexis.a".parse()?), Some(&2));
    assert_eq!(summary.by_rule.get(&"lexis.b".parse()?), Some(&1));
    assert_eq!(summary.by_rule.get(&"ost.c".parse()?), Some(&1));
    let law = summary.by_tag.get("law").ok_or("no law tag")?;
    assert_eq!(law.get("5"), Some(&3));
    assert_eq!(law.get("7"), Some(&1));
    let set = summary.by_tag.get("set").ok_or("no set tag")?;
    assert_eq!(set.get("LEXIS"), Some(&2));
    Ok(())
}

#[test]
fn summary_of_no_findings_is_empty() -> Outcome {
    let report = Report::new(run()?, Vec::new());
    assert_eq!(report.summary, Summary::default());
    assert_eq!(report.summary.total, 0);
    Ok(())
}

#[test]
fn summary_collects_from_a_finding_iterator() -> Outcome {
    let findings = findings()?;
    let summary: Summary = findings.iter().collect();
    assert_eq!(summary, Report::new(run()?, findings).summary);
    Ok(())
}

#[test]
fn summary_severity_map_serialises_to_json_with_kebab_keys() -> Outcome {
    let report = Report::new(run()?, findings()?);
    let json = serde_json::to_value(&report.summary)?;
    assert_eq!(json["by_severity"], json!({"error": 3, "warning": 1}));
    let map: BTreeMap<Severity, usize> = serde_json::from_value(json["by_severity"].clone())?;
    assert_eq!(map, report.summary.by_severity);
    Ok(())
}

#[test]
fn report_round_trips_through_json() -> Outcome {
    let report = Report::new(run()?, findings()?);
    let back: Report = serde_json::from_str(&serde_json::to_string(&report)?)?;
    assert_eq!(back, report);
    Ok(())
}

#[test]
fn run_round_trips_and_keeps_selectors_as_text() -> Outcome {
    let run = run()?;
    let json = serde_json::to_value(&run)?;
    assert_eq!(json["ignore"], json!(["law=prompts", "lexis.loop"]));
    assert_eq!(json["date"], json!("2026-11-27T10:00:00Z"));
    let back: Run = serde_json::from_value(json)?;
    assert_eq!(back, run);
    Ok(())
}

#[test]
fn run_refuses_bad_date_and_unknown_field() {
    let mut value =
        json!({"tool": "1", "date": "yesterday", "catalogues": [], "select": [], "ignore": [], "paths": []});
    assert!(serde_json::from_value::<Run>(value.clone()).is_err());
    value["date"] = json!("2026-11-27T10:00:00Z");
    value["extra"] = json!(1);
    assert!(serde_json::from_value::<Run>(value).is_err());
}

#[test]
fn report_refuses_unknown_field() -> Outcome {
    let mut value = serde_json::to_value(Report::new(run()?, Vec::new()))?;
    value["extra"] = json!(1);
    assert!(serde_json::from_value::<Report>(value).is_err());
    Ok(())
}

#[test]
fn span_round_trips_and_refuses_unknown_field() -> Outcome {
    let span: Span = serde_json::from_value(
        json!({"start": {"line": 2, "column": 3}, "end": {"line": 2, "column": 9}, "bytes": {"start": 10, "end": 16}}),
    )?;
    let back: Span = serde_json::from_str(&serde_json::to_string(&span)?)?;
    assert_eq!(back, span);
    assert!(
        serde_json::from_value::<Span>(
            json!({"start": {"line": 2, "column": 3, "x": 1}, "end": {"line": 2, "column": 9}, "bytes": {"start": 1, "end": 2}})
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn profile_round_trips_and_refuses_unknown_field() -> Outcome {
    let profile: Profile = serde_json::from_value(json!({"kind": "engine", "home": ["src/*/engines/*.py"]}))?;
    let back: Profile = serde_json::from_str(&serde_json::to_string(&profile)?)?;
    assert_eq!(back, profile);
    assert!(serde_json::from_value::<Profile>(json!({"kind": "engine", "home": [], "extra": 1})).is_err());
    assert!(serde_json::from_value::<Profile>(json!({"kind": "Engine", "home": []})).is_err());
    Ok(())
}

#[test]
fn catalogue_round_trips_through_yaml_and_json() -> Outcome {
    let catalogue: Catalogue = serde_json::from_value(json!({
        "name": "eager",
        "version": "1.0.0",
        "rules": [{
            "id": "lexis.elif", "message": "m",
            "check": {"engine": "pattern", "rule": {"kind": "elif_clause"}},
            "fix": {"safety": "none", "reason": "design"},
        }],
        "profiles": [{"kind": "engine", "home": ["src/*/engines/*.py"]}],
    }))?;
    let back: Catalogue = serde_json::from_str(&serde_json::to_string(&catalogue)?)?;
    assert_eq!(back, catalogue);
    let yaml: Catalogue = serde_saphyr::from_str(&serde_saphyr::to_string(&catalogue)?)?;
    assert_eq!(yaml, catalogue);
    Ok(())
}
