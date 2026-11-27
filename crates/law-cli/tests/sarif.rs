mod common;

use std::fs;

use common::{ELIF, MIXED_CODE, MIXED_FILE, Outcome, Project, WITH_ELIF, mixed};
use jsonschema::Validator;
use serde_json::{Value, json};

/// The official SARIF 2.1.0 schema, as the OASIS technical committee publishes it (draft-04), fetched once:
/// <https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json>
const SCHEMA_URL: &str =
    "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json";
const SCHEMA_SHA256: &str = "c3b4bb2d6093897483348925aaa73af03b3e3f4bd4ca38cef26dcb4212a2682e";
const SCHEMA: &str = include_str!("fixtures/sarif-schema-2.1.0.json");

type Fault = Box<dyn std::error::Error>;

fn validator() -> Result<Validator, Fault> {
    Ok(jsonschema::validator_for(&serde_json::from_str(SCHEMA)?)?)
}

fn invalid(validator: &Validator, log: &Value) -> String {
    validator
        .iter_errors(log)
        .map(|error| format!("{} at {}", error, error.instance_path()))
        .collect::<Vec<_>>()
        .join("; ")
}

fn sarif(project: &Project, args: &[&str]) -> Result<Value, Fault> {
    let mut all = vec!["--output-format", "sarif"];
    all.extend_from_slice(args);
    Ok(serde_json::from_str(&project.check(&all)?.stdout)?)
}

fn decoded(uri: &str) -> Result<String, Fault> {
    let bytes = uri.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'%' => {
                out.push(u8::from_str_radix(
                    std::str::from_utf8(bytes.get(at + 1..at + 3).ok_or("short escape")?)?,
                    16,
                )?);
                at += 3;
            }
            byte => {
                out.push(byte);
                at += 1;
            }
        }
    }
    Ok(String::from_utf8(out)?)
}

fn results(log: &Value) -> Result<&Vec<Value>, Fault> {
    Ok(log["runs"][0]["results"].as_array().ok_or("no results")?)
}

#[test]
fn sarif_schema_fixture_is_the_one_published() -> Outcome {
    assert!(SCHEMA_URL.ends_with("sarif-schema-2.1.0.json"));
    assert_eq!(SCHEMA_SHA256.len(), 64);
    let schema: Value = serde_json::from_str(SCHEMA)?;
    assert_eq!(
        schema["id"],
        "https://docs.oasis-open.org/sarif/sarif/v2.1.0/errata01/os/schemas/sarif-schema-2.1.0.json"
    );
    Ok(())
}

#[test]
fn sarif_the_validator_refuses_what_is_not_a_log() -> Outcome {
    let validator = validator()?;
    for (name, log) in [
        ("no version", json!({"runs": []})),
        ("a wrong version", json!({"version": "2.0.0", "runs": []})),
        ("a run without its tool", json!({"version": "2.1.0", "runs": [{}]})),
        (
            "a level outside the enum",
            json!({"version": "2.1.0", "runs": [{"tool": {"driver": {"name": "x"}}, "results": [{"message": {"text": "m"}, "level": "fatal"}]}]}),
        ),
    ] {
        assert!(!validator.is_valid(&log), "{name}");
    }
    Ok(())
}

#[test]
fn sarif_validates_against_the_official_schema() -> Outcome {
    let validator = validator()?;
    let clean = Project::with_rules("[\"src\"]", ELIF)?;
    clean.file("src/a.py", "x = 1\n")?;
    let warned = Project::with_rules(
        "[\"src\"]",
        &ELIF
            .replace("t.elif", "t.warn")
            .replace("fix: {safety: none", "severity: warning\n  fix: {safety: none"),
    )?;
    warned.file("src/a.py", WITH_ELIF)?;
    for (name, project) in [("mixed with fixes", mixed()?), ("clean", clean), ("a warning", warned)] {
        let log = sarif(&project, &[])?;
        assert!(validator.is_valid(&log), "{name}: {}", invalid(&validator, &log));
    }
    Ok(())
}

#[test]
fn sarif_a_sampled_log_validates_with_the_driver_and_tool_facts() -> Outcome {
    let log = sarif(&mixed()?, &[])?;
    assert_eq!(log["version"], "2.1.0");
    assert_eq!(log["$schema"], "https://json.schemastore.org/sarif-2.1.0.json");
    let driver = &log["runs"][0]["tool"]["driver"];
    assert_eq!(driver["name"], "law");
    assert_eq!(driver["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(log["runs"][0]["columnKind"], "unicodeCodePoints");
    Ok(())
}

#[test]
fn sarif_results_round_trip_the_findings() -> Outcome {
    let project = mixed()?;
    let log = sarif(&project, &[])?;
    let report = project.json(&[])?;
    let findings = report["findings"].as_array().ok_or("no findings")?;
    let results = results(&log)?;
    assert_eq!(results.len(), findings.len());
    assert_eq!(results.len(), 28);
    for (result, finding) in results.iter().zip(findings) {
        assert_eq!(result["ruleId"], finding["rule"]);
        assert_eq!(result["message"]["text"], finding["message"]);
        assert_eq!(result["level"], finding["severity"]);
        let region = &result["locations"][0]["physicalLocation"]["region"];
        assert_eq!(region["startLine"], finding["span"]["start"]["line"]);
        assert_eq!(region["startColumn"], finding["span"]["start"]["column"]);
        assert_eq!(region["endLine"], finding["span"]["end"]["line"]);
        assert_eq!(region["endColumn"], finding["span"]["end"]["column"]);
        assert_eq!(
            decoded(
                result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"]
                    .as_str()
                    .ok_or("uri")?
            )?,
            finding["path"].as_str().ok_or("path")?
        );
    }
    Ok(())
}

#[test]
fn sarif_rules_are_the_ones_that_fired_with_their_message_tags_and_level() -> Outcome {
    let project = mixed()?;
    let log = sarif(&project, &[])?;
    let rules = log["runs"][0]["tool"]["driver"]["rules"].as_array().ok_or("no rules")?;
    let ids: Vec<&str> = rules.iter().filter_map(|rule| rule["id"].as_str()).collect();
    assert_eq!(ids, ["t.elif", "t.lambda", "t.swap", "t.untagged"]);
    let by = |id: &str| rules.iter().find(|rule| rule["id"] == id).ok_or(id.to_owned());
    assert_eq!(by("t.swap")?["shortDescription"]["text"], "use new");
    assert_eq!(
        by("t.swap")?["properties"]["tags"],
        json!(["kit=py", "law=5", "set=LEXIS"])
    );
    assert_eq!(
        by("t.elif")?["properties"]["tags"],
        json!(["law=10", "set=LEXIS", "set=OST"])
    );
    assert_eq!(by("t.lambda")?["defaultConfiguration"]["level"], "warning");
    assert_eq!(by("t.elif")?["defaultConfiguration"]["level"], "error");
    assert_eq!(by("t.untagged")?["properties"]["tags"], json!([]));
    let fired: Vec<&str> = results(&log)?
        .iter()
        .filter_map(|result| result["ruleId"].as_str())
        .collect();
    assert!(fired.iter().all(|id| ids.contains(id)));
    Ok(())
}

#[test]
fn sarif_a_fix_replaces_the_bytes_the_finding_covers_in_the_source() -> Outcome {
    let project = mixed()?;
    let log = sarif(&project, &[])?;
    let report = project.json(&[])?;
    let fixed: Vec<&Value> = results(&log)?
        .iter()
        .filter(|result| result.get("fixes").is_some())
        .collect();
    assert_eq!(fixed.len(), 1, "only a finding with a fix carries fixes");
    let fix = &fixed[0]["fixes"][0]["artifactChanges"][0];
    let replacement = &fix["replacements"][0];
    let offset = usize::try_from(replacement["deletedRegion"]["byteOffset"].as_u64().ok_or("offset")?)?;
    let length = usize::try_from(replacement["deletedRegion"]["byteLength"].as_u64().ok_or("length")?)?;
    let source = fs::read(project.path().join(MIXED_FILE))?;
    assert_eq!(&source[offset..offset + length], "old(\u{e1}.b)".as_bytes());
    assert_eq!(replacement["insertedContent"]["text"], "new(\u{e1}.b)");
    assert_eq!(
        decoded(fix["artifactLocation"]["uri"].as_str().ok_or("uri")?)?,
        MIXED_FILE
    );
    let edit = &report["findings"][0]["fix"]["edits"][0];
    assert_eq!(edit["bytes"]["start"], replacement["deletedRegion"]["byteOffset"]);
    assert_eq!(edit["text"], replacement["insertedContent"]["text"]);
    let mut applied = source.clone();
    applied.splice(
        offset..offset + length,
        replacement["insertedContent"]["text"].as_str().ok_or("text")?.bytes(),
    );
    assert_eq!(String::from_utf8(applied)?, MIXED_CODE.replace("old(", "new("));
    Ok(())
}

#[test]
fn sarif_columns_count_code_points_not_bytes() -> Outcome {
    let project = mixed()?;
    let log = sarif(&project, &[])?;
    let swap = results(&log)?
        .iter()
        .find(|result| result["ruleId"] == "t.swap")
        .ok_or("no swap")?;
    let region = &swap["locations"][0]["physicalLocation"]["region"];
    assert_eq!(
        (region["startColumn"].as_u64(), region["endColumn"].as_u64()),
        (Some(5), Some(13))
    );
    let line = MIXED_CODE.lines().nth(2).ok_or("line")?;
    assert_eq!(line.chars().count(), 12);
    assert_eq!(line.len(), 13);
    Ok(())
}

#[test]
fn sarif_uris_are_relative_forward_slashed_and_percent_encoded() -> Outcome {
    let project = mixed()?;
    project.file("pk/a b/c%d.py", "print(1)\n")?;
    let log = sarif(&project, &[])?;
    let uris: Vec<&str> = results(&log)?
        .iter()
        .filter_map(|result| result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"].as_str())
        .collect();
    assert!(uris.contains(&"pk/dir%20%C3%B1/%C3%B1and%C3%BA.py"), "{uris:?}");
    assert!(uris.contains(&"pk/a%20b/c%25d.py"), "{uris:?}");
    for uri in uris {
        assert!(
            !uri.starts_with('/') && !uri.contains('\\') && !uri.contains(".."),
            "{uri}"
        );
        assert!(uri.is_ascii() && !uri.contains(' '), "{uri}");
        assert!(project.path().join(decoded(uri)?).is_file(), "{uri}");
    }
    Ok(())
}

#[test]
fn sarif_a_clean_run_is_a_valid_log_with_no_results() -> Outcome {
    let project = Project::with_rules("[\"src\"]", ELIF)?;
    project.file("src/a.py", "x = 1\n")?;
    let out = project.check(&["--output-format", "sarif"])?;
    assert_eq!(out.code, 0);
    let log: Value = serde_json::from_str(&out.stdout)?;
    assert_eq!(results(&log)?.len(), 0);
    assert_eq!(log["runs"][0]["tool"]["driver"]["rules"], json!([]));
    Ok(())
}

#[test]
fn sarif_exit_code_follows_the_findings_like_every_format() -> Outcome {
    let project = mixed()?;
    let sarif = project.check(&["--output-format", "sarif"])?;
    let text = project.check(&[])?;
    assert_eq!((sarif.code, sarif.stderr.as_str()), (text.code, ""));
    assert_eq!(sarif.code, 1);
    Ok(())
}
