mod common;

use std::path::{Path, PathBuf};

use common::{Outcome, RULE, manifest, rule, write};
use law_config::{Fault, catalogue};
use law_model::{Check, FixPolicy, Refusal, Rule};

fn sample_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eager")
}

fn faults_of(text: &str) -> Result<Vec<Fault>, Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir()?;
    let dir = manifest(tmp.path(), "c")?;
    write(tmp.path(), "c/rules/x.yml", text)?;
    catalogue(&dir).err().ok_or_else(|| "the catalogue loaded".into())
}

fn bad(body: &str) -> String {
    format!("- id: a.b\n  message: m\n{body}")
}

const CHECK: &str = "  check: {engine: pattern, rule: {kind: x}}\n";
const FIX: &str = "  fix: {safety: safe}\n";

#[test]
fn catalogue_sample_loads_from_directory() -> Outcome {
    let loaded = catalogue(&sample_dir()).map_err(|faults| format!("{faults:?}"))?;
    assert_eq!((loaded.name.as_str(), loaded.version.as_str()), ("eager", "1.0.0"));
    assert_eq!(loaded.rules.len(), 1);
    assert_eq!(loaded.profiles.len(), 1);
    let rule = loaded.rules.first().ok_or("no rule")?;
    assert_eq!(rule.id.as_str(), "lexis.elif");
    assert!(rule.tags.contains("law", "5") && rule.tags.contains("principle", "dispatch"));
    assert!(matches!(rule.check, Check::Pattern(_)));
    assert!(matches!(rule.fix, FixPolicy::None { .. }));
    assert_eq!(rule.cases.invalid.len(), 1);
    Ok(())
}

#[test]
fn catalogue_sample_round_trips() -> Outcome {
    let loaded = catalogue(&sample_dir()).map_err(|faults| format!("{faults:?}"))?;
    let yaml = serde_saphyr::to_string(&loaded.rules)?;
    let back: Vec<Rule> = serde_saphyr::from_str(&yaml)?;
    assert_eq!(back, loaded.rules);
    let json: Vec<Rule> = serde_json::from_str(&serde_json::to_string(&loaded.rules)?)?;
    assert_eq!(json, loaded.rules);
    Ok(())
}

#[test]
fn catalogue_reads_nested_rule_files_in_a_stable_order() -> Outcome {
    let tmp = tempfile::tempdir()?;
    let dir = manifest(tmp.path(), "c")?;
    write(tmp.path(), "c/rules/b/deep.yml", &rule("b.deep"))?;
    write(tmp.path(), "c/rules/a.yml", &rule("a.top"))?;
    write(tmp.path(), "c/rules/ignored.txt", "not yaml: [")?;
    let ids = |dir: &Path| -> Result<Vec<String>, Box<dyn std::error::Error>> {
        Ok(catalogue(dir)
            .map_err(|faults| format!("{faults:?}"))?
            .rules
            .iter()
            .map(|rule| rule.id.to_string())
            .collect())
    };
    let first = ids(&dir)?;
    assert_eq!(first, ["a.top", "b.deep"]);
    assert_eq!(first, ids(&dir)?);
    Ok(())
}

#[test]
fn catalogue_without_rule_files_is_empty_not_a_fault() -> Outcome {
    let tmp = tempfile::tempdir()?;
    let dir = manifest(tmp.path(), "c")?;
    let loaded = catalogue(&dir).map_err(|faults| format!("{faults:?}"))?;
    assert!(loaded.rules.is_empty() && loaded.profiles.is_empty());
    Ok(())
}

#[test]
fn catalogue_without_manifest_is_an_io_fault_naming_it() -> Outcome {
    let tmp = tempfile::tempdir()?;
    write(tmp.path(), "c/rules/x.yml", RULE)?;
    let faults = catalogue(&tmp.path().join("c")).err().ok_or("loaded")?;
    assert!(
        matches!(faults.as_slice(), [Fault::Io { path, .. }] if path.ends_with("catalogue.toml")),
        "{faults:?}"
    );
    Ok(())
}

#[test]
fn catalogue_manifest_refuses_unknown_key() -> Outcome {
    let tmp = tempfile::tempdir()?;
    write(
        tmp.path(),
        "c/catalogue.toml",
        "name = \"c\"\nversion = \"1\"\ndescription = \"d\"\nextra = 1\n",
    )?;
    let faults = catalogue(&tmp.path().join("c")).err().ok_or("loaded")?;
    assert!(
        matches!(faults.as_slice(), [Fault::Parse { path, line: Some(4), .. }] if path.ends_with("catalogue.toml")),
        "{faults:?}"
    );
    Ok(())
}

#[test]
fn catalogue_refuses_bad_id_as_invalid_bad_id_with_path_and_line() -> Outcome {
    for id in ["Elif", "lexis", "lexis."] {
        let faults = faults_of(&format!("- id: {id}\n  message: m\n{CHECK}{FIX}"))?;
        assert!(
            matches!(faults.as_slice(), [Fault::Invalid { path, line: Some(1), invalid }]
                if path.ends_with("rules/x.yml") && invalid.refusal == Refusal::BadId),
            "{id}: {faults:?}"
        );
    }
    Ok(())
}

#[test]
fn catalogue_refuses_multiline_message() -> Outcome {
    let faults = faults_of(&format!("- id: a.b\n  message: \"a\\nb\"\n{CHECK}{FIX}"))?;
    assert!(
        matches!(faults.as_slice(), [Fault::Invalid { invalid, line: Some(2), .. }] if invalid.refusal == Refusal::MultilineMessage),
        "{faults:?}"
    );
    Ok(())
}

#[test]
fn catalogue_refuses_bad_glob_in_scope() -> Outcome {
    let faults = faults_of(&format!(
        "- id: a.b\n  message: m\n  scope: {{include: [\"a/[\"]}}\n{CHECK}{FIX}"
    ))?;
    assert!(
        matches!(faults.as_slice(), [Fault::Invalid { invalid, .. }] if invalid.refusal == Refusal::BadGlob),
        "{faults:?}"
    );
    Ok(())
}

#[test]
fn catalogue_refuses_missing_fix_as_parse_naming_the_field() -> Outcome {
    let faults = faults_of(&bad(CHECK))?;
    assert!(
        matches!(faults.as_slice(), [Fault::Parse { message, line: Some(_), .. }] if message.contains("fix")),
        "{faults:?}"
    );
    Ok(())
}

#[test]
fn catalogue_refuses_none_without_reason() -> Outcome {
    let faults = faults_of(&bad(&format!("{CHECK}  fix: {{safety: none}}\n")))?;
    assert!(
        matches!(faults.as_slice(), [Fault::Parse { message, .. }] if message.contains("reason")),
        "{faults:?}"
    );
    Ok(())
}

#[test]
fn catalogue_refuses_unknown_field() -> Outcome {
    let faults = faults_of(&bad(&format!("  extra: 1\n{CHECK}{FIX}")))?;
    assert!(
        matches!(faults.as_slice(), [Fault::Parse { message, line: Some(3), .. }] if message.contains("extra")),
        "{faults:?}"
    );
    Ok(())
}

#[test]
fn catalogue_refuses_unknown_engine() -> Outcome {
    let faults = faults_of(&bad(&format!("  check: {{engine: magic, rule: {{}}}}\n{FIX}")))?;
    assert!(
        matches!(faults.as_slice(), [Fault::Parse { message, .. }] if message.contains("magic")),
        "{faults:?}"
    );
    Ok(())
}

#[test]
fn catalogue_refuses_unknown_key_inside_each_engine_check() -> Outcome {
    for check in [
        "{engine: pattern, rule: {kind: x}, extra: 1}",
        "{engine: tree, dirs: [\"a/**\"], extra: 1}",
        "{engine: shape, extra: 1}",
        "{engine: shape, classes: {count: 1, extra: 1}}",
    ] {
        let faults = faults_of(&bad(&format!("  check: {check}\n{FIX}")))?;
        assert!(
            matches!(faults.as_slice(), [Fault::Parse { message, path, .. }]
                if message.contains("extra") && path.ends_with("rules/x.yml")),
            "{check}: {faults:?}"
        );
    }
    Ok(())
}

#[test]
fn catalogue_refuses_unknown_field_in_every_nested_level() -> Outcome {
    for (name, body) in [
        ("scope", "  scope: {include: [\"a/*.py\"], extra: 1}\n"),
        ("cases", "  cases: {valid: [], extra: []}\n"),
        ("fixed", "  cases: {fixed: [{code: x, expect: y, extra: 1}]}\n"),
    ] {
        let faults = faults_of(&bad(&format!("{body}{CHECK}{FIX}")))?;
        assert!(
            matches!(faults.as_slice(), [Fault::Parse { message, .. }] if message.contains("extra")),
            "{name}: {faults:?}"
        );
    }
    Ok(())
}

#[test]
fn catalogue_refuses_unknown_field_in_a_profile() -> Outcome {
    let tmp = tempfile::tempdir()?;
    let dir = manifest(tmp.path(), "c")?;
    write(
        tmp.path(),
        "c/profiles/p.yml",
        "- kind: engine\n  home: []\n  extra: 1\n",
    )?;
    let faults = catalogue(&dir).err().ok_or("loaded")?;
    assert!(
        matches!(faults.as_slice(), [Fault::Parse { path, .. }] if path.ends_with("profiles/p.yml")),
        "{faults:?}"
    );
    Ok(())
}

#[test]
fn fault_line_at_top_level_is_the_line_of_the_refused_value() -> Outcome {
    let text = format!("{}\n- id: Bad\n  message: m\n{CHECK}{FIX}", rule("a.b"));
    let faults = faults_of(&text)?;
    assert!(
        matches!(faults.as_slice(), [Fault::Invalid { line: Some(6), .. }]),
        "{faults:?}"
    );
    Ok(())
}

#[test]
fn fault_line_inside_check_points_into_the_check_block() -> Outcome {
    let text = "- id: a.b\n  message: m\n  check:\n    engine: pattern\n    rule: {kind: x}\n    extra: 1\n  fix: {safety: safe}\n";
    let faults = faults_of(text)?;
    let line = match faults.as_slice() {
        [Fault::Parse { line, .. }] => *line,
        other => return Err(format!("{other:?}").into()),
    };
    let found = line.ok_or("no line")?;
    assert!(
        (3..=6).contains(&found),
        "line {found} is outside the check block (lines 3-6)"
    );
    Ok(())
}

#[test]
fn fault_display_is_path_then_what() -> Outcome {
    let faults = faults_of(&format!("- id: Elif\n  message: m\n{CHECK}{FIX}"))?;
    let text = faults.first().ok_or("none")?.to_string();
    assert!(text.contains("rules/x.yml:1: bad-id"), "{text}");
    Ok(())
}

#[test]
fn fault_two_bad_files_are_both_returned() -> Outcome {
    let tmp = tempfile::tempdir()?;
    let dir = manifest(tmp.path(), "c")?;
    write(
        tmp.path(),
        "c/rules/one.yml",
        &bad(&format!("  extra: 1\n{CHECK}{FIX}")),
    )?;
    write(
        tmp.path(),
        "c/rules/two.yml",
        &format!("- id: Elif\n  message: m\n{CHECK}{FIX}"),
    )?;
    let faults = catalogue(&dir).err().ok_or("loaded")?;
    assert_eq!(faults.len(), 2, "{faults:?}");
    let mentions = |name: &str| faults.iter().any(|fault| fault.to_string().contains(name));
    assert!(mentions("one.yml") && mentions("two.yml"), "{faults:?}");
    Ok(())
}

#[test]
fn fault_manifest_and_rule_file_are_both_returned() -> Outcome {
    let tmp = tempfile::tempdir()?;
    write(
        tmp.path(),
        "c/rules/x.yml",
        &format!("- id: Elif\n  message: m\n{CHECK}{FIX}"),
    )?;
    let faults = catalogue(&tmp.path().join("c")).err().ok_or("loaded")?;
    assert_eq!(faults.len(), 2, "{faults:?}");
    Ok(())
}

#[test]
fn fault_two_bad_rules_in_one_file_are_both_returned() -> Outcome {
    let text = format!("- id: Elif\n  message: m\n{CHECK}{FIX}- id: a.b\n  message: \"x\\ny\"\n{CHECK}{FIX}");
    let faults = faults_of(&text)?;
    assert_eq!(faults.len(), 2, "two bad rules in one file; faults: {faults:?}");
    Ok(())
}

#[test]
fn fault_duplicate_rule_across_two_files_names_both() -> Outcome {
    let tmp = tempfile::tempdir()?;
    let dir = manifest(tmp.path(), "c")?;
    write(tmp.path(), "c/rules/a.yml", &rule("a.same"))?;
    write(tmp.path(), "c/rules/b.yml", &rule("a.same"))?;
    let faults = catalogue(&dir).err().ok_or("loaded")?;
    assert!(
        matches!(faults.as_slice(), [Fault::DuplicateRule { id, first, second }]
            if id.as_str() == "a.same" && first.ends_with("a.yml") && second.ends_with("b.yml")),
        "{faults:?}"
    );
    Ok(())
}

#[test]
fn fault_duplicate_rule_inside_one_file_is_reported() -> Outcome {
    let faults = faults_of(&format!("{}{}", rule("a.same"), rule("a.same")))?;
    assert!(matches!(faults.as_slice(), [Fault::DuplicateRule { .. }]), "{faults:?}");
    Ok(())
}

#[test]
fn fault_duplicate_profile_kind_is_reported() -> Outcome {
    let tmp = tempfile::tempdir()?;
    let dir = manifest(tmp.path(), "c")?;
    write(tmp.path(), "c/profiles/a.yml", "- kind: engine\n  home: [\"a/*.py\"]\n")?;
    write(tmp.path(), "c/profiles/b.yml", "- kind: engine\n  home: [\"b/*.py\"]\n")?;
    let faults = catalogue(&dir).err().ok_or("loaded")?;
    assert!(
        matches!(faults.as_slice(), [Fault::DuplicateProfile { kind, .. }] if kind.as_str() == "engine"),
        "{faults:?}"
    );
    Ok(())
}

#[test]
fn fault_yaml_syntax_error_carries_path_and_line() -> Outcome {
    let faults = faults_of("- id: a.b\n  message: [\n")?;
    assert!(
        matches!(faults.as_slice(), [Fault::Parse { path, line: Some(_), .. }] if path.ends_with("rules/x.yml")),
        "{faults:?}"
    );
    Ok(())
}

#[test]
fn catalogue_refuses_unknown_field_in_a_placed_case() -> Outcome {
    let faults = faults_of(&bad(&format!(
        "  cases: {{valid: [{{code: x, extra: 1}}]}}\n{CHECK}{FIX}"
    )))?;
    assert!(
        matches!(faults.as_slice(), [Fault::Parse { line: Some(_), .. }]),
        "{faults:?}"
    );
    Ok(())
}

fn with_prose(prose: &str) -> Result<Result<law_model::Catalogue, Vec<Fault>>, Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir()?;
    let dir = manifest(tmp.path(), "c")?;
    write(
        tmp.path(),
        "c/catalogue.toml",
        &format!("name = \"c\"\nversion = \"1\"\ndescription = \"d\"\n{prose}"),
    )?;
    write(tmp.path(), "c/rules/x.yml", RULE)?;
    Ok(catalogue(&dir))
}

#[test]
fn catalogue_prose_entries_are_read_with_their_tags_and_text() -> Outcome {
    let loaded = with_prose(
        "[[prose]]\ntags = {law = \"6\", set = \"METRON\"}\ntext = \"a\"\n\n[[prose]]\ntags = {law = \"12\"}\ntext = \"b\"\n",
    )?
    .map_err(|faults| format!("{faults:?}"))?;
    assert_eq!(loaded.prose.len(), 2);
    assert!(loaded.prose[0].tags.contains("law", "6") && loaded.prose[0].tags.contains("set", "METRON"));
    assert_eq!(
        (loaded.prose[0].text.as_str(), loaded.prose[1].text.as_str()),
        ("a", "b")
    );
    Ok(())
}

#[test]
fn catalogue_without_prose_has_none() -> Outcome {
    let loaded = with_prose("")?.map_err(|faults| format!("{faults:?}"))?;
    assert!(loaded.prose.is_empty());
    Ok(())
}

#[test]
fn catalogue_prose_refuses_an_unknown_field_and_a_missing_text_or_tags() -> Outcome {
    for (name, prose) in [
        (
            "an unknown field",
            "[[prose]]\ntags = {law = \"1\"}\ntext = \"a\"\nwhy = \"b\"\n",
        ),
        ("no text", "[[prose]]\ntags = {law = \"1\"}\n"),
        ("no tags", "[[prose]]\ntext = \"a\"\n"),
        ("a text that is not text", "[[prose]]\ntags = {law = \"1\"}\ntext = 3\n"),
    ] {
        assert!(with_prose(prose)?.is_err(), "{name}");
    }
    Ok(())
}
