mod common;

use std::path::{Path, PathBuf};

use common::{Outcome, manifest, rule, write};
use law_config::{Bundle, Fault, Origin, Project, load};
use law_model::Refusal;

fn project(root: &Path, config: &str) -> Result<Project, Box<dyn std::error::Error>> {
    let path = write(root, "law.toml", config)?;
    Project::read(&path)?.ok_or_else(|| "no project".into())
}

fn faults_of(
    root: &Path,
    config: &str,
    builtins: &[(&str, PathBuf)],
) -> Result<Vec<Fault>, Box<dyn std::error::Error>> {
    let project = project(root, config)?;
    let lookup = |name: &str| {
        builtins
            .iter()
            .find(|(known, _)| *known == name)
            .map(|(_, dir)| Origin::Dir(dir.clone()))
    };
    load(&project, &lookup).err().ok_or_else(|| "loaded".into())
}

fn kinded(id: &str, kind: &str) -> String {
    format!(
        "- id: {id}\n  message: m\n  scope: {{kind: {kind}}}\n  check: {{engine: pattern, rule: {{kind: x}}}}\n  fix: {{safety: safe}}\n"
    )
}

#[test]
fn load_layers_catalogues_in_order_and_reads_exceptions() -> Outcome {
    let tmp = tempfile::tempdir()?;
    let eager = manifest(tmp.path(), "eager")?;
    write(tmp.path(), "eager/rules/a.yml", &rule("a.one"))?;
    manifest(tmp.path(), "mine")?;
    write(tmp.path(), "mine/rules/b.yml", &rule("b.two"))?;
    write(
        tmp.path(),
        "ex.toml",
        "[[exception]]\nrule = \"a.one\"\npath = \"app/legacy/**\"\nreason = \"r\"\napproved = \"a\"\n",
    )?;
    let project = project(
        tmp.path(),
        "catalogues = [\"eager\", \"mine/\"]\nexceptions = \"ex.toml\"\n",
    )?;
    let lookup = |name: &str| (name == "eager").then(|| Origin::Dir(eager.clone()));
    let loaded = load(&project, &lookup).map_err(|faults| format!("{faults:?}"))?;
    let order: Vec<&str> = loaded
        .catalogues
        .iter()
        .map(|catalogue| catalogue.name.as_str())
        .collect();
    assert_eq!(order, ["eager", "mine"]);
    assert_eq!(loaded.exceptions.len(), 1);
    assert_eq!(loaded.project, project);
    Ok(())
}

#[test]
fn load_without_catalogues_or_exceptions_is_empty() -> Outcome {
    let tmp = tempfile::tempdir()?;
    let project = project(tmp.path(), "")?;
    let loaded = load(&project, &|_| None).map_err(|faults| format!("{faults:?}"))?;
    assert!(loaded.catalogues.is_empty() && loaded.exceptions.is_empty());
    Ok(())
}

#[test]
fn load_duplicate_rule_across_two_catalogues_is_a_fault_never_an_override() -> Outcome {
    let tmp = tempfile::tempdir()?;
    manifest(tmp.path(), "first")?;
    write(tmp.path(), "first/rules/a.yml", &rule("a.same"))?;
    manifest(tmp.path(), "second")?;
    write(tmp.path(), "second/rules/a.yml", &rule("a.same"))?;
    let faults = faults_of(tmp.path(), "catalogues = [\"first/\", \"second/\"]\n", &[])?;
    assert!(
        matches!(faults.as_slice(), [Fault::DuplicateRule { id, first, second }]
            if id.as_str() == "a.same" && first.ends_with("first") && second.ends_with("second")),
        "{faults:?}"
    );
    Ok(())
}

fn renamed(id: &str, former: &str) -> String {
    format!(
        "- id: {id}\n  message: m\n  formerly: [{former}]\n  check: {{engine: pattern, rule: {{kind: x}}}}\n  fix: {{safety: safe}}\n"
    )
}

#[test]
fn load_a_former_id_claimed_by_two_rules_of_one_catalogue_is_a_fault() -> Outcome {
    let tmp = tempfile::tempdir()?;
    manifest(tmp.path(), "first")?;
    write(
        tmp.path(),
        "first/rules/a.yml",
        &format!("{}{}", renamed("a.one", "old-one"), renamed("a.two", "old-one")),
    )?;
    let faults = faults_of(tmp.path(), "catalogues = [\"first/\"]\n", &[])?;
    assert!(
        matches!(faults.as_slice(), [Fault::DuplicateFormer { id, first, second, .. }]
            if id.as_str() == "old-one" && first.as_str() == "a.one" && second.as_str() == "a.two"),
        "{faults:?}"
    );
    Ok(())
}

#[test]
fn load_a_former_id_claimed_in_two_catalogues_is_a_fault() -> Outcome {
    let tmp = tempfile::tempdir()?;
    manifest(tmp.path(), "first")?;
    write(tmp.path(), "first/rules/a.yml", &renamed("a.one", "old-one"))?;
    manifest(tmp.path(), "second")?;
    write(tmp.path(), "second/rules/a.yml", &renamed("b.two", "old-one"))?;
    let faults = faults_of(tmp.path(), "catalogues = [\"first/\", \"second/\"]\n", &[])?;
    assert!(
        matches!(faults.as_slice(), [Fault::DuplicateFormer { id, first, second, path }]
            if id.as_str() == "old-one" && first.as_str() == "a.one" && second.as_str() == "b.two"
                && path.ends_with("second")),
        "{faults:?}"
    );
    assert!(faults[0].to_string().contains("former id `old-one`"), "{faults:?}");
    Ok(())
}

#[test]
fn load_a_former_id_listed_twice_by_one_rule_is_a_fault() -> Outcome {
    let tmp = tempfile::tempdir()?;
    manifest(tmp.path(), "first")?;
    write(tmp.path(), "first/rules/a.yml", &renamed("a.one", "old-one, old-one"))?;
    let faults = faults_of(tmp.path(), "catalogues = [\"first/\"]\n", &[])?;
    assert!(
        matches!(faults.as_slice(), [Fault::DuplicateFormer { .. }]),
        "{faults:?}"
    );
    Ok(())
}

#[test]
fn load_distinct_former_ids_load() -> Outcome {
    let tmp = tempfile::tempdir()?;
    manifest(tmp.path(), "first")?;
    write(
        tmp.path(),
        "first/rules/a.yml",
        &format!("{}{}", renamed("a.one", "old-one"), renamed("a.two", "old-two")),
    )?;
    let project = project(tmp.path(), "catalogues = [\"first/\"]\n")?;
    let loaded = load(&project, &|_| None).map_err(|faults| format!("{faults:?}"))?;
    assert_eq!(loaded.catalogues[0].rules.len(), 2);
    Ok(())
}

#[test]
fn load_duplicate_profile_kind_across_two_catalogues_is_a_fault() -> Outcome {
    let tmp = tempfile::tempdir()?;
    for dir in ["first", "second"] {
        manifest(tmp.path(), dir)?;
        write(
            tmp.path(),
            &format!("{dir}/profiles/p.yml"),
            "- kind: engine\n  home: [\"a/*.py\"]\n",
        )?;
    }
    let faults = faults_of(tmp.path(), "catalogues = [\"first/\", \"second/\"]\n", &[])?;
    assert!(
        matches!(faults.as_slice(), [Fault::DuplicateProfile { kind, .. }] if kind.as_str() == "engine"),
        "{faults:?}"
    );
    Ok(())
}

#[test]
fn load_two_duplicates_in_one_load_are_both_returned() -> Outcome {
    let tmp = tempfile::tempdir()?;
    manifest(tmp.path(), "first")?;
    write(
        tmp.path(),
        "first/rules/a.yml",
        &format!("{}{}", rule("a.one"), rule("a.two")),
    )?;
    manifest(tmp.path(), "second")?;
    write(
        tmp.path(),
        "second/rules/a.yml",
        &format!("{}{}", rule("a.one"), rule("a.two")),
    )?;
    let faults = faults_of(tmp.path(), "catalogues = [\"first/\", \"second/\"]\n", &[])?;
    let dups = faults
        .iter()
        .filter(|fault| matches!(fault, Fault::DuplicateRule { .. }))
        .count();
    assert_eq!(dups, 2, "{faults:?}");
    Ok(())
}

#[test]
fn load_faults_of_two_catalogues_are_all_returned() -> Outcome {
    let tmp = tempfile::tempdir()?;
    manifest(tmp.path(), "first")?;
    write(
        tmp.path(),
        "first/rules/a.yml",
        "- id: Elif\n  message: m\n  check: {engine: pattern, rule: {kind: x}}\n  fix: {safety: safe}\n",
    )?;
    manifest(tmp.path(), "second")?;
    write(tmp.path(), "second/rules/b.yml", "- id: a.b\n  extra: 1\n")?;
    let faults = faults_of(
        tmp.path(),
        "catalogues = [\"first/\", \"second/\"]\nexceptions = \"missing.toml\"\n",
        &[],
    )?;
    assert_eq!(faults.len(), 3, "{faults:?}");
    assert!(
        faults
            .iter()
            .any(|fault| matches!(fault, Fault::Invalid { invalid, .. } if invalid.refusal == Refusal::BadId))
    );
    assert!(faults.iter().any(|fault| matches!(fault, Fault::Parse { .. })));
    assert!(
        faults
            .iter()
            .any(|fault| matches!(fault, Fault::Io { path, .. } if path.ends_with("missing.toml")))
    );
    Ok(())
}

#[test]
fn load_unknown_builtin_name_is_a_fault_naming_it() -> Outcome {
    let tmp = tempfile::tempdir()?;
    let faults = faults_of(tmp.path(), "catalogues = [\"nowhere\"]\n", &[])?;
    assert!(
        matches!(faults.as_slice(), [Fault::UnknownCatalogue { name, path }] if name == "nowhere" && path.ends_with("law.toml")),
        "{faults:?}"
    );
    Ok(())
}

#[test]
fn load_missing_catalogue_directory_is_a_fault() -> Outcome {
    let tmp = tempfile::tempdir()?;
    let faults = faults_of(tmp.path(), "catalogues = [\"absent/\"]\n", &[])?;
    assert!(
        matches!(faults.as_slice(), [Fault::Io { path, .. }] if path.ends_with("catalogue.toml")),
        "{faults:?}"
    );
    Ok(())
}

#[test]
fn load_rule_kind_no_profile_declares_is_unknown_kind() -> Outcome {
    let tmp = tempfile::tempdir()?;
    manifest(tmp.path(), "c")?;
    write(tmp.path(), "c/rules/a.yml", &kinded("a.one", "engine"))?;
    let faults = faults_of(tmp.path(), "catalogues = [\"c/\"]\n", &[])?;
    assert!(
        matches!(faults.as_slice(), [Fault::UnknownKind { kind, .. }] if kind.as_str() == "engine"),
        "{faults:?}"
    );
    Ok(())
}

#[test]
fn load_rule_kind_declared_by_a_later_catalogue_resolves() -> Outcome {
    let tmp = tempfile::tempdir()?;
    manifest(tmp.path(), "c")?;
    write(tmp.path(), "c/rules/a.yml", &kinded("a.one", "engine"))?;
    manifest(tmp.path(), "d")?;
    write(tmp.path(), "d/profiles/p.yml", "- kind: engine\n  home: [\"a/*.py\"]\n")?;
    let project = project(tmp.path(), "catalogues = [\"c/\", \"d/\"]\n")?;
    assert!(load(&project, &|_| None).is_ok());
    Ok(())
}

#[test]
fn load_threshold_override_on_an_undeclared_kind_names_kind_and_threshold() -> Outcome {
    let tmp = tempfile::tempdir()?;
    let faults = faults_of(tmp.path(), "[profiles.x.thresholds]\nlines = 400\n", &[])?;
    assert!(
        faults
            .iter()
            .any(|fault| matches!(fault, Fault::UnknownKind { kind, .. } if kind.as_str() == "x")),
        "{faults:?}"
    );
    assert!(
        faults
            .iter()
            .any(|fault| matches!(fault, Fault::UnknownThreshold { kind, name, .. }
            if kind.as_str() == "x" && name == "lines")),
        "{faults:?}"
    );
    Ok(())
}

#[test]
fn load_threshold_override_on_a_declared_kind_naming_an_undeclared_threshold() -> Outcome {
    let tmp = tempfile::tempdir()?;
    manifest(tmp.path(), "c")?;
    write(tmp.path(), "c/profiles/p.yml", "- kind: engine\n  home: [\"a/*.py\"]\n")?;
    let faults = faults_of(
        tmp.path(),
        "catalogues = [\"c/\"]\n[profiles.engine.thresholds]\nlines = 400\nbranches = 9\n",
        &[],
    )?;
    let names: Vec<&str> = faults
        .iter()
        .filter_map(|fault| match fault {
            Fault::UnknownThreshold { kind, name, .. } if kind.as_str() == "engine" => Some(name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(names, ["branches", "lines"], "{faults:?}");
    assert!(
        !faults.iter().any(|fault| matches!(fault, Fault::UnknownKind { .. })),
        "{faults:?}"
    );
    Ok(())
}

#[test]
fn load_two_undeclared_thresholds_are_both_returned() -> Outcome {
    let tmp = tempfile::tempdir()?;
    manifest(tmp.path(), "c")?;
    write(tmp.path(), "c/profiles/p.yml", "- kind: engine\n  home: [\"a/*.py\"]\n")?;
    write(tmp.path(), "c/profiles/q.yml", "- kind: logic\n  home: [\"b/*.py\"]\n")?;
    let faults = faults_of(
        tmp.path(),
        "catalogues = [\"c/\"]\n[profiles.engine.thresholds]\nlines = 1\n[profiles.logic.thresholds]\nlines = 1\n",
        &[],
    )?;
    assert_eq!(faults.len(), 2, "{faults:?}");
    Ok(())
}

#[test]
fn load_exception_without_target_is_a_named_fault_with_path_and_line() -> Outcome {
    let tmp = tempfile::tempdir()?;
    write(
        tmp.path(),
        "ex.toml",
        "[[exception]]\nrule = \"a.one\"\nreason = \"r\"\napproved = \"a\"\n",
    )?;
    let faults = faults_of(tmp.path(), "exceptions = \"ex.toml\"\n", &[])?;
    assert!(
        matches!(faults.as_slice(), [Fault::Invalid { invalid, path, line: Some(_) }]
            if invalid.refusal == Refusal::ExceptionWithoutTarget && path.ends_with("ex.toml")),
        "{faults:?}"
    );
    Ok(())
}

#[test]
fn load_exception_with_empty_reason_or_approver_is_empty_text() -> Outcome {
    for field in ["reason", "approved"] {
        let tmp = tempfile::tempdir()?;
        let (reason, approved) = if field == "reason" { ("", "a") } else { ("r", "") };
        write(
            tmp.path(),
            "ex.toml",
            &format!(
                "[[exception]]\nrule = \"a.one\"\npath = \"x/**\"\nreason = \"{reason}\"\napproved = \"{approved}\"\n"
            ),
        )?;
        let faults = faults_of(tmp.path(), "exceptions = \"ex.toml\"\n", &[])?;
        assert!(
            matches!(faults.as_slice(), [Fault::Invalid { invalid, .. }] if invalid.refusal == Refusal::EmptyText),
            "{field}: {faults:?}"
        );
    }
    Ok(())
}

#[test]
fn load_exception_with_bad_date_is_a_parse_fault() -> Outcome {
    let tmp = tempfile::tempdir()?;
    write(
        tmp.path(),
        "ex.toml",
        "[[exception]]\nrule = \"a.one\"\npath = \"x/**\"\nreason = \"r\"\napproved = \"a\"\nexpires = \"2026-13-45\"\n",
    )?;
    let faults = faults_of(tmp.path(), "exceptions = \"ex.toml\"\n", &[])?;
    assert!(
        matches!(faults.as_slice(), [Fault::Parse { path, .. }] if path.ends_with("ex.toml")),
        "{faults:?}"
    );
    Ok(())
}

#[test]
fn load_exceptions_file_refuses_unknown_key() -> Outcome {
    let tmp = tempfile::tempdir()?;
    write(tmp.path(), "ex.toml", "extra = 1\n")?;
    let faults = faults_of(tmp.path(), "exceptions = \"ex.toml\"\n", &[])?;
    assert!(matches!(faults.as_slice(), [Fault::Parse { .. }]), "{faults:?}");
    Ok(())
}

#[test]
fn load_two_bad_exceptions_in_one_file_are_both_returned() -> Outcome {
    let tmp = tempfile::tempdir()?;
    write(
        tmp.path(),
        "ex.toml",
        "[[exception]]\nrule = \"a.one\"\nreason = \"r\"\napproved = \"a\"\n[[exception]]\nrule = \"a.two\"\nreason = \"r\"\napproved = \"a\"\n",
    )?;
    let faults = faults_of(tmp.path(), "exceptions = \"ex.toml\"\n", &[])?;
    assert_eq!(faults.len(), 2, "two bad exceptions in one file; faults: {faults:?}");
    Ok(())
}

#[test]
fn load_fault_display_starts_with_the_path() -> Outcome {
    let tmp = tempfile::tempdir()?;
    write(tmp.path(), "ex.toml", "extra = 1\n")?;
    let faults = faults_of(tmp.path(), "exceptions = \"ex.toml\"\n", &[])?;
    let text = faults.first().ok_or("none")?.to_string();
    assert!(
        text.starts_with(&tmp.path().join("ex.toml").display().to_string()),
        "{text}"
    );
    Ok(())
}

fn bundle(name: &str, files: &[(&str, &str)]) -> Origin {
    Origin::Bundle(Bundle {
        name: name.to_owned(),
        files: files
            .iter()
            .map(|(path, text)| (PathBuf::from(path), (*text).to_owned()))
            .collect(),
    })
}

const MANIFEST_TEXT: &str = "name = \"eager\"\nversion = \"1\"\ndescription = \"d\"\n";

fn bundled_faults(files: &[(&str, &str)]) -> Result<Vec<Fault>, Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir()?;
    let project = project(tmp.path(), "catalogues = [\"eager\"]\n")?;
    let origin = bundle("eager", files);
    let lookup = |name: &str| (name == "eager").then(|| origin.clone());
    load(&project, &lookup).err().ok_or_else(|| "loaded".into())
}

#[test]
fn load_a_bundle_and_a_directory_of_the_same_files_load_the_same_catalogue() -> Outcome {
    let tmp = tempfile::tempdir()?;
    let dir = manifest(tmp.path(), "eager")?;
    write(tmp.path(), "eager/rules/a.yml", &rule("a.one"))?;
    write(tmp.path(), "eager/rules/deep/b.yml", &rule("b.two"))?;
    write(
        tmp.path(),
        "eager/profiles/p.yml",
        "- kind: engine\n  home: [\"a/*.py\"]\n",
    )?;
    let files = [
        ("catalogue.toml", MANIFEST_TEXT),
        ("rules/a.yml", &rule("a.one")),
        ("rules/deep/b.yml", &rule("b.two")),
        ("profiles/p.yml", "- kind: engine\n  home: [\"a/*.py\"]\n"),
    ];
    let project = project(tmp.path(), "catalogues = [\"eager\"]\n")?;
    let from_dir = load(&project, &|_| Some(Origin::Dir(dir.clone()))).map_err(|faults| format!("{faults:?}"))?;
    let origin = bundle("eager", &files);
    let from_bundle = load(&project, &|_| Some(origin.clone())).map_err(|faults| format!("{faults:?}"))?;
    assert_eq!(from_bundle.catalogues, from_dir.catalogues);
    assert_eq!(from_bundle.catalogues[0].rules.len(), 2);
    Ok(())
}

#[test]
fn load_a_bundle_resolves_each_name_the_builtins_resolver_gives() -> Outcome {
    let tmp = tempfile::tempdir()?;
    let project = project(tmp.path(), "catalogues = [\"eager\"]\n")?;
    let origin = bundle(
        "eager",
        &[("catalogue.toml", MANIFEST_TEXT), ("rules/a.yml", &rule("a.one"))],
    );
    let loaded =
        load(&project, &|name| (name == "eager").then(|| origin.clone())).map_err(|faults| format!("{faults:?}"))?;
    assert_eq!(loaded.catalogues[0].name, "eager");
    assert_eq!(loaded.catalogues[0].rules[0].id.as_str(), "a.one");
    Ok(())
}

#[test]
fn load_a_fault_inside_a_bundled_rule_file_is_placed_by_the_bundles_name() -> Outcome {
    let faults = bundled_faults(&[
        ("catalogue.toml", MANIFEST_TEXT),
        ("rules/lexis.yml", "- id: a.b\n  extra: 1\n"),
    ])?;
    let [Fault::Parse { path, .. }] = faults.as_slice() else {
        return Err(format!("{faults:?}").into());
    };
    assert_eq!(path, &PathBuf::from("eager:rules/lexis.yml"));
    assert!(
        faults[0].to_string().starts_with("eager:rules/lexis.yml:"),
        "{}",
        faults[0]
    );
    Ok(())
}

#[test]
fn load_a_fault_inside_a_bundled_manifest_or_profile_is_placed_by_the_bundles_name() -> Outcome {
    let manifest = bundled_faults(&[("catalogue.toml", "name = 1\n")])?;
    assert!(
        manifest
            .iter()
            .all(|fault| fault.to_string().starts_with("eager:catalogue.toml")),
        "{manifest:?}"
    );
    let profile = bundled_faults(&[
        ("catalogue.toml", MANIFEST_TEXT),
        ("profiles/p.yml", "- kind: engine\n"),
    ])?;
    assert!(
        profile
            .iter()
            .any(|fault| fault.to_string().starts_with("eager:profiles/p.yml")),
        "{profile:?}"
    );
    Ok(())
}

#[test]
fn load_a_bundled_invalid_rule_names_its_place_and_line() -> Outcome {
    let faults = bundled_faults(&[
        ("catalogue.toml", MANIFEST_TEXT),
        (
            "rules/ost.yml",
            "- id: Elif\n  message: m\n  check: {engine: pattern, rule: {kind: x}}\n  fix: {safety: safe}\n",
        ),
    ])?;
    assert!(
        matches!(faults.as_slice(), [Fault::Invalid { path, line: Some(_), .. }] if path == &PathBuf::from("eager:rules/ost.yml")),
        "{faults:?}"
    );
    Ok(())
}

#[test]
fn load_a_bundle_without_its_manifest_is_a_fault_naming_it() -> Outcome {
    let faults = bundled_faults(&[("rules/a.yml", &rule("a.one"))])?;
    assert!(
        faults
            .iter()
            .any(|fault| fault.to_string().contains("eager:catalogue.toml")),
        "{faults:?}"
    );
    Ok(())
}

#[test]
fn load_a_rule_declared_in_a_bundle_and_in_a_directory_is_a_duplicate_naming_both_places() -> Outcome {
    let tmp = tempfile::tempdir()?;
    manifest(tmp.path(), "mine")?;
    write(tmp.path(), "mine/rules/a.yml", &rule("a.same"))?;
    let project = project(tmp.path(), "catalogues = [\"eager\", \"mine/\"]\n")?;
    let origin = bundle(
        "eager",
        &[("catalogue.toml", MANIFEST_TEXT), ("rules/a.yml", &rule("a.same"))],
    );
    let faults = load(&project, &|name| (name == "eager").then(|| origin.clone()))
        .err()
        .ok_or("loaded")?;
    assert!(
        matches!(faults.as_slice(), [Fault::DuplicateRule { id, first, second }]
            if id.as_str() == "a.same" && first == &PathBuf::from("eager") && second.ends_with("mine")),
        "{faults:?}"
    );
    Ok(())
}

#[test]
fn load_a_builtin_name_the_resolver_does_not_know_is_still_unknown() -> Outcome {
    let tmp = tempfile::tempdir()?;
    let project = project(tmp.path(), "catalogues = [\"eager\", \"other\"]\n")?;
    let origin = bundle("eager", &[("catalogue.toml", MANIFEST_TEXT)]);
    let faults = load(&project, &|name| (name == "eager").then(|| origin.clone()))
        .err()
        .ok_or("loaded")?;
    assert!(
        matches!(faults.as_slice(), [Fault::UnknownCatalogue { name, .. }] if name == "other"),
        "{faults:?}"
    );
    Ok(())
}

fn tach(id: &str, check: &str) -> String {
    format!("- id: {id}\n  message: m\n  check: {check}\n  fix: {{safety: none, reason: ported}}\n")
}

fn indistinct_of(rules: &[(&str, &str)]) -> Result<Vec<Fault>, Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir()?;
    manifest(tmp.path(), "mine")?;
    write(
        tmp.path(),
        "mine/rules/b.yml",
        &rules.iter().map(|(id, check)| tach(id, check)).collect::<String>(),
    )?;
    let project = project(tmp.path(), "catalogues = [\"mine/\"]\n")?;
    Ok(load(&project, &|_| None).err().unwrap_or_default())
}

#[test]
fn load_two_tach_rules_of_one_kind_or_two_without_a_kind_are_indistinct() -> Outcome {
    for (a, b, key) in [
        (
            "{engine: tach, code: LayerViolation}",
            "{engine: tach, code: LayerViolation}",
            "tach `LayerViolation`",
        ),
        ("{engine: tach}", "{engine: tach}", "tach"),
    ] {
        let faults = indistinct_of(&[("t.one", a), ("t.two", b)])?;
        assert!(
            matches!(faults.as_slice(), [Fault::Indistinct { key: found, first, second, .. }]
                if found == key && first.as_str() == "t.one" && second.as_str() == "t.two"),
            "{key}: {faults:?}"
        );
        assert!(
            faults[0]
                .to_string()
                .contains(&format!("rules `t.one` and `t.two` both take every {key} diagnostic")),
            "{}",
            faults[0]
        );
    }
    Ok(())
}

#[test]
fn load_tach_rules_of_different_kinds_or_one_named_beside_an_unnamed_are_distinct() -> Outcome {
    for rules in [
        [
            ("t.one", "{engine: tach, code: LayerViolation}"),
            ("t.two", "{engine: tach, code: UndeclaredDependency}"),
        ],
        [
            ("t.one", "{engine: tach, code: LayerViolation}"),
            ("t.two", "{engine: tach}"),
        ],
    ] {
        assert_eq!(indistinct_of(&rules)?, [], "{rules:?}");
    }
    Ok(())
}

#[test]
fn load_a_tach_rule_and_a_ty_rule_with_nothing_named_are_not_indistinct() -> Outcome {
    assert_eq!(
        indistinct_of(&[("t.one", "{engine: tach}"), ("t.two", "{engine: ty}")])?,
        []
    );
    Ok(())
}

#[test]
fn load_a_tach_code_that_is_not_a_word_still_loads_here_and_is_judged_by_the_engine() -> Outcome {
    assert_eq!(indistinct_of(&[("t.one", "{engine: tach, code: Nope}")])?, []);
    Ok(())
}

fn profiled(id: &str, kinds: &str, aspect: &str) -> String {
    format!(
        "- id: {id}\n  message: m\n  check: {{engine: profile, kinds: [{kinds}], aspect: {aspect}}}\n  fix: {{safety: none, reason: x}}\n"
    )
}

const ASPECTS: [&str; 7] = ["present", "surname", "methods", "attributes", "line", "bases", "never"];

#[test]
fn load_profile_rule_judging_an_aspect_its_profile_gives_no_data_for_is_unfed() -> Outcome {
    for aspect in ASPECTS {
        let tmp = tempfile::tempdir()?;
        manifest(tmp.path(), "c")?;
        write(tmp.path(), "c/profiles/p.yml", "- kind: unit\n  home: [\"a/*.py\"]\n")?;
        write(tmp.path(), "c/rules/a.yml", &profiled("a.one", "unit", aspect))?;
        let faults = faults_of(tmp.path(), "catalogues = [\"c/\"]\n", &[])?;
        assert!(
            matches!(faults.as_slice(), [Fault::UnfedAspect { rule, kind, aspect: found, .. }]
                if rule.as_str() == "a.one" && kind.as_str() == "unit" && found.name() == aspect),
            "{aspect}: {faults:?}"
        );
    }
    Ok(())
}

#[test]
fn load_profile_rule_over_a_profile_that_feeds_its_aspect_loads() -> Outcome {
    let tmp = tempfile::tempdir()?;
    manifest(tmp.path(), "c")?;
    write(
        tmp.path(),
        "c/profiles/p.yml",
        "- kind: unit\n  home: [\"a/*.py\"]\n  suffix: Unit\n  methods: [go]\n  attributes: [spec]\n  line: [typing.Protocol]\n  bases: [typing.Protocol]\n  never: [abc.ABC]\n",
    )?;
    for (index, aspect) in ASPECTS.iter().enumerate() {
        write(
            tmp.path(),
            &format!("c/rules/r{index}.yml"),
            &profiled(&format!("a.r{index}"), "unit", aspect),
        )?;
    }
    let project = project(tmp.path(), "catalogues = [\"c/\"]\n")?;
    let loaded = load(&project, &|_| None).map_err(|faults| format!("{faults:?}"))?;
    assert_eq!(loaded.catalogues[0].rules.len(), 7);
    Ok(())
}

#[test]
fn load_profile_rule_over_two_kinds_names_only_the_kind_that_is_unfed() -> Outcome {
    let tmp = tempfile::tempdir()?;
    manifest(tmp.path(), "c")?;
    write(
        tmp.path(),
        "c/profiles/p.yml",
        "- kind: unit\n  home: [\"a/*.py\"]\n  methods: [go]\n",
    )?;
    write(tmp.path(), "c/profiles/q.yml", "- kind: part\n  home: [\"b/*.py\"]\n")?;
    write(tmp.path(), "c/rules/a.yml", &profiled("a.one", "unit, part", "methods"))?;
    let faults = faults_of(tmp.path(), "catalogues = [\"c/\"]\n", &[])?;
    assert!(
        matches!(faults.as_slice(), [Fault::UnfedAspect { kind, .. }] if kind.as_str() == "part"),
        "{faults:?}"
    );
    Ok(())
}

#[test]
fn load_profile_rule_over_a_kind_no_profile_declares_is_unknown_kind_beside_a_later_declaration() -> Outcome {
    let tmp = tempfile::tempdir()?;
    manifest(tmp.path(), "c")?;
    write(tmp.path(), "c/rules/a.yml", &profiled("a.one", "ghost", "present"))?;
    let faults = faults_of(tmp.path(), "catalogues = [\"c/\"]\n", &[])?;
    assert!(
        matches!(faults.as_slice(), [Fault::UnknownKind { kind, .. }] if kind.as_str() == "ghost"),
        "{faults:?}"
    );
    manifest(tmp.path(), "d")?;
    write(
        tmp.path(),
        "d/profiles/p.yml",
        "- kind: ghost\n  home: [\"a/*.py\"]\n  suffix: Ghost\n",
    )?;
    let project = project(tmp.path(), "catalogues = [\"c/\", \"d/\"]\n")?;
    assert!(load(&project, &|_| None).is_ok());
    Ok(())
}

#[test]
fn load_profile_rule_with_no_kinds_is_refused_as_empty_kinds_at_its_file() -> Outcome {
    let tmp = tempfile::tempdir()?;
    manifest(tmp.path(), "c")?;
    write(tmp.path(), "c/rules/a.yml", &profiled("a.one", "", "present"))?;
    let faults = faults_of(tmp.path(), "catalogues = [\"c/\"]\n", &[])?;
    assert!(
        matches!(faults.as_slice(), [Fault::Invalid { invalid, path, line: Some(_) }]
            if invalid.refusal == Refusal::EmptyKinds && path.ends_with("a.yml")),
        "{faults:?}"
    );
    Ok(())
}

#[test]
fn load_threshold_override_of_a_declared_threshold_loads() -> Outcome {
    let tmp = tempfile::tempdir()?;
    manifest(tmp.path(), "c")?;
    write(
        tmp.path(),
        "c/profiles/p.yml",
        "- kind: unit\n  home: [\"a/*.py\"]\n  thresholds: {lines: 400, depth: 3}\n",
    )?;
    let project = project(
        tmp.path(),
        "catalogues = [\"c/\"]\n[profiles.unit.thresholds]\nlines = 120\ndepth = 2\n",
    )?;
    let loaded = load(&project, &|_| None).map_err(|faults| format!("{faults:?}"))?;
    let thresholds = &loaded.catalogues[0].profiles[0].thresholds;
    assert_eq!(thresholds.get("lines"), Some(&400));
    assert_eq!(thresholds.get("depth"), Some(&3));
    Ok(())
}
