mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use common::{Outcome, fixture};
use law_external::{
    TachCommand, TachDetails, TachDiagnostic, TachDocument, TachError, TachKind, TachLocated, TachSeverity,
};
use serde_json::{Value, json};

const CHECK: &str = ".check.out";
const EXTERNAL: &str = ".ext.err";

fn document(name: &str) -> Result<TachDocument, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(&fixture(&format!("tach/{name}{CHECK}"))?)?)
}

fn diagnostics(name: &str) -> Result<Vec<TachDiagnostic>, Box<dyn std::error::Error>> {
    match document(name)? {
        TachDocument::Diagnostics(found) => Ok(found),
        TachDocument::Error(error) => Err(format!("{name}: an error document: {error:?}").into()),
    }
}

fn error(name: &str) -> Result<TachError, Box<dyn std::error::Error>> {
    match document(name)? {
        TachDocument::Error(error) => Ok(error),
        TachDocument::Diagnostics(found) => Err(format!("{name}: diagnostics: {found:?}").into()),
    }
}

fn external(name: &str) -> Result<Vec<TachDiagnostic>, Box<dyn std::error::Error>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/tach")
        .join(format!("{name}{EXTERNAL}"));
    let text = fs::read_to_string(path)?;
    TachDiagnostic::external(&text).map_err(|line| format!("{name}: `{line}` is no kind").into())
}

fn names(map: &[(&str, &str)]) -> BTreeMap<String, String> {
    map.iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}

fn located(found: &TachDiagnostic) -> Option<&TachLocated> {
    match found {
        TachDiagnostic::Located(located) => Some(located),
        TachDiagnostic::Global(_) => None,
    }
}

fn kind(found: &TachDiagnostic) -> &TachKind {
    match found.details() {
        TachDetails::Code(kind) | TachDetails::Configuration(kind) => kind,
    }
}

fn grown(document: &mut Value) {
    let extra = || json!({"added_by_a_newer_tach": {"x": [1, 2]}});
    let top = document.as_object_mut().map(|object| {
        object.insert("added_by_a_newer_tach".to_owned(), extra());
    });
    if top.is_some() {
        return;
    }
    document
        .as_array_mut()
        .into_iter()
        .flatten()
        .filter_map(|diagnostic| diagnostic.as_object_mut()?.values_mut().next())
        .for_each(|body| {
            if let Some(object) = body.as_object_mut() {
                object.insert("added_by_a_newer_tach".to_owned(), extra());
            }
        });
}

#[test]
fn boundary_check_document_of_each_captured_internal_kind_parses() -> Outcome {
    type Row<'a> = (&'a str, &'a str, &'a str, u32, &'a [(&'a str, &'a str)]);
    let table: [Row; 6] = [
        (
            "undeclared",
            "UndeclaredDependency",
            "src/pkg/ui/view.py",
            1,
            &[
                ("dependency", "pkg.low.base"),
                ("usage_module", "pkg.ui"),
                ("definition_module", "pkg.low"),
            ],
        ),
        (
            "layer",
            "LayerViolation",
            "src/pkg/core/model.py",
            2,
            &[
                ("dependency", "pkg.ui.view"),
                ("usage_module", "pkg.core"),
                ("usage_layer", "core"),
                ("definition_module", "pkg.ui"),
                ("definition_layer", "ui"),
            ],
        ),
        (
            "forbidden",
            "ForbiddenDependency",
            "src/pkg/ui/view.py",
            1,
            &[
                ("dependency", "pkg.low.base"),
                ("usage_module", "pkg.ui"),
                ("definition_module", "pkg.low"),
            ],
        ),
        (
            "interfaces",
            "PrivateDependency",
            "src/pkg/ui/view.py",
            1,
            &[
                ("dependency", "pkg.core.model.x"),
                ("definition_module", "pkg.core"),
                ("usage_module", "pkg.ui"),
            ],
        ),
        ("ignores", "MissingIgnoreDirectiveReason", "src/pkg/ui/view.py", 1, &[]),
        ("ignores", "UnusedIgnoreDirective", "src/pkg/ui/view.py", 1, &[]),
    ];
    for (name, expected, file, line, fields) in table {
        let found = diagnostics(name)?;
        let hit = found
            .iter()
            .find(|found| kind(found).name == expected)
            .ok_or_else(|| format!("{name}: no {expected} in {found:?}"))?;
        let at = located(hit).ok_or_else(|| format!("{name}: {expected} is not located"))?;
        assert_eq!(
            (at.file_path.as_path(), at.line_number, at.severity),
            (Path::new(file), line, TachSeverity::Error),
            "{name}"
        );
        assert_eq!(kind(hit).fields, names(fields), "{name} {expected}");
        assert!(matches!(at.details, TachDetails::Code(_)), "{name}");
    }
    Ok(())
}

#[test]
fn boundary_check_document_clean_is_empty_and_an_unconfigured_project_is_a_global_warning() -> Outcome {
    assert_eq!(diagnostics("ok")?, []);
    let found = diagnostics("base")?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].severity(), TachSeverity::Warning);
    assert!(matches!(found[0], TachDiagnostic::Global(_)));
    assert!(matches!(found[0].details(), TachDetails::Configuration(_)));
    let nothing = kind(&found[0]);
    assert_eq!(
        (nothing.name.as_str(), nothing.fields.len()),
        ("NoFirstPartyImportsFound", 0)
    );
    assert!(nothing.configuration());
    Ok(())
}

#[test]
fn boundary_check_document_unknown_layer_is_a_global_configuration_error() -> Outcome {
    let found = diagnostics("unknownlayer")?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].severity(), TachSeverity::Error);
    assert!(matches!(found[0].details(), TachDetails::Configuration(_)));
    assert_eq!(kind(&found[0]).name, "UnknownLayer");
    assert_eq!(kind(&found[0]).fields, names(&[("layer", "nope")]));
    assert_eq!(kind(&found[0]).line().as_deref(), Some("UnknownLayer, layer nope"));
    Ok(())
}

#[test]
fn boundary_check_document_error_objects_stand_for_one_synthetic_kind_each() -> Outcome {
    let cycle = error("cycle")?;
    assert_eq!(cycle.error, "Circular dependency");
    assert_eq!(cycle.dependencies, ["pkg.core", "pkg.low"]);
    let found = cycle.diagnostics().ok_or("a cycle has diagnostics")?;
    assert_eq!(found.len(), 1);
    assert!(matches!(found[0], TachDiagnostic::Global(_)));
    assert_eq!(found[0].severity(), TachSeverity::Error);
    assert_eq!(kind(&found[0]).name, "CircularDependency");
    assert_eq!(kind(&found[0]).fields, names(&[("modules", "pkg.core, pkg.low")]));
    assert_eq!(
        kind(&found[0]).line().as_deref(),
        Some("pkg.core, pkg.low depend on each other")
    );
    let visibility = error("visibility")?.diagnostics().ok_or("visibility has diagnostics")?;
    assert_eq!(visibility.len(), 1);
    assert_eq!(kind(&visibility[0]).name, "Visibility");
    assert_eq!(
        kind(&visibility[0]).fields,
        names(&[
            ("dependent", "pkg.ui"),
            ("dependency", "pkg.core"),
            ("visibility", "pkg.low")
        ])
    );
    assert_eq!(
        kind(&visibility[0]).line().as_deref(),
        Some("pkg.ui depends on pkg.core, outside its visibility (pkg.low)")
    );
    Ok(())
}

#[test]
fn boundary_check_document_error_objects_unknown_to_e_law_stand_for_nothing() -> Outcome {
    let other: TachDocument = serde_json::from_str("{\"error\": \"Something else\"}")?;
    let TachDocument::Error(other) = other else {
        return Err("not an error".into());
    };
    assert_eq!(other.diagnostics(), None);
    let several: TachDocument = serde_json::from_str(
        "{\"error\": \"Visibility error\", \"visibility_errors\": [[\"a\", \"b\", [\"c\", \"d\"]], [\"e\", \"f\", []]]}",
    )?;
    let TachDocument::Error(several) = several else {
        return Err("not an error".into());
    };
    let found = several.diagnostics().ok_or("diagnostics")?;
    assert_eq!(found.len(), 2);
    assert_eq!(kind(&found[0]).fields["visibility"], "c, d");
    assert_eq!(kind(&found[1]).fields["visibility"], "");
    Ok(())
}

#[test]
fn boundary_check_document_with_unknown_extra_fields_still_parses_as_before() -> Outcome {
    for name in [
        "base",
        "undeclared",
        "layer",
        "forbidden",
        "cycle",
        "visibility",
        "interfaces",
        "ignores",
        "unknownlayer",
    ] {
        let text = fixture(&format!("tach/{name}{CHECK}"))?;
        let plain: TachDocument = serde_json::from_str(&text)?;
        let mut value: Value = serde_json::from_str(&text)?;
        grown(&mut value);
        assert_ne!(
            value,
            serde_json::from_str::<Value>(&text)?,
            "{name}: nothing was added"
        );
        let again: TachDocument = serde_json::from_value(value)?;
        assert_eq!(again, plain, "{name}");
    }
    Ok(())
}

#[test]
fn boundary_check_document_extra_field_of_a_kind_is_kept_as_one_more_field() -> Outcome {
    let mut value: Value = serde_json::from_str(&fixture("tach/undeclared.check.out")?)?;
    value[0]["Located"]["details"]["Code"]["UndeclaredDependency"]["added_by_a_newer_tach"] = json!("x");
    let TachDocument::Diagnostics(found) = serde_json::from_value(value)? else {
        return Err("not a list".into());
    };
    assert_eq!(kind(&found[0]).name, "UndeclaredDependency");
    assert_eq!(kind(&found[0]).fields["added_by_a_newer_tach"], "x");
    assert_eq!(
        kind(&found[0]).line().as_deref(),
        Some("pkg.ui imports pkg.low.base, undeclared")
    );
    Ok(())
}

#[test]
fn boundary_check_document_that_is_neither_a_list_nor_an_error_object_does_not_parse() {
    for text in [
        "\"x\"",
        "7",
        "null",
        "{}",
        "{\"Located\": {}}",
        "[{\"Nowhere\": {}}]",
        "[{\"Global\": {\"severity\": \"Fatal\", \"details\": {\"Code\": {\"A\": []}}}}]",
        "[{\"Global\": {\"severity\": \"Error\", \"details\": {\"Code\": {}}}}]",
        "[{\"Global\": {\"severity\": \"Error\", \"details\": {\"Code\": {\"A\": [], \"B\": []}}}}]",
        "[{\"Global\": {\"severity\": \"Error\", \"details\": {\"Other\": {\"A\": []}}}}]",
        "[{\"Located\": {\"severity\": \"Error\", \"details\": {\"Code\": {\"A\": []}}}}]",
    ] {
        assert!(serde_json::from_str::<TachDocument>(text).is_err(), "{text}");
    }
}

#[test]
fn boundary_kind_fields_are_text_whatever_json_type_tach_wrote() -> Result<(), Box<dyn std::error::Error>> {
    let found: TachDocument = serde_json::from_str(
        "[{\"Global\": {\"severity\": \"Error\", \"details\": {\"Code\": {\"K\": {\"n\": 3, \"s\": \"x\", \"l\": [\"a\"], \"z\": null}}}}}]",
    )?;
    let TachDocument::Diagnostics(found) = found else {
        return Err("not a list".into());
    };
    assert_eq!(
        kind(&found[0]).fields,
        names(&[("n", "3"), ("s", "x"), ("l", "[\"a\"]"), ("z", "null")])
    );
    Ok(())
}

#[test]
fn boundary_external_text_of_each_captured_stderr_is_read_line_by_line() -> Outcome {
    let found = external("ext1")?;
    assert_eq!(found.len(), 2);
    assert!(matches!(found[0], TachDiagnostic::Global(_)));
    assert_eq!(found[0].severity(), TachSeverity::Error);
    assert_eq!(kind(&found[0]).name, "UnusedExternalDependency");
    assert_eq!(
        kind(&found[0]).fields,
        names(&[("package_module_name", "httpx"), ("package_name", "pkg")])
    );
    let at = located(&found[1]).ok_or("located")?;
    assert_eq!(
        (at.file_path.as_path(), at.line_number, at.severity),
        (Path::new("src/pkg/core/model.py"), 1, TachSeverity::Error)
    );
    assert_eq!(kind(&found[1]).name, "UndeclaredExternalDependency");
    assert_eq!(
        kind(&found[1]).fields,
        names(&[("dependency", "requests"), ("package_name", "pkg")])
    );
    let unused = external("unusedext")?;
    assert_eq!(unused.len(), 1);
    assert_eq!(unused[0].severity(), TachSeverity::Warning);
    assert_eq!(kind(&unused[0]).name, "UnusedExternalDependency");
    let module = external("modext")?;
    let named: Vec<(&str, u32, &BTreeMap<String, String>)> = module
        .iter()
        .map(|found| {
            Some((
                kind(found).name.as_str(),
                located(found)?.line_number,
                &kind(found).fields,
            ))
        })
        .collect::<Option<_>>()
        .ok_or("located")?;
    assert_eq!(
        named,
        [
            (
                "ModuleUndeclaredExternalDependency",
                1,
                &names(&[("usage_module", "pkg.core"), ("dependency", "requests")])
            ),
            (
                "ModuleForbiddenExternalDependency",
                2,
                &names(&[("usage_module", "pkg.core"), ("dependency", "yaml")])
            ),
        ]
    );
    let ignored = external("ignores")?;
    assert_eq!(ignored.len(), 1);
    assert_eq!(kind(&ignored[0]).name, "UnusedIgnoreDirective");
    assert_eq!(located(&ignored[0]).map(|at| at.line_number), Some(3));
    Ok(())
}

#[test]
fn boundary_external_text_with_no_diagnostic_line_is_empty_for_every_other_capture() -> Outcome {
    for name in [
        "base",
        "ok",
        "undeclared",
        "layer",
        "forbidden",
        "interfaces",
        "unknownlayer",
        "nocfg",
    ] {
        assert_eq!(external(name)?, [], "{name}");
    }
    Ok(())
}

#[test]
fn boundary_external_text_unknown_line_is_the_error_and_names_the_line() {
    for line in [
        "[FAIL] src/pkg/core/model.py:1: Something new.",
        "[FAIL] Error: Something new.",
        "[WARN] Warning: Something new.",
        "[FAIL] src/pkg/core/model.py: no line number.",
        "[FAIL] Warning: External package 'httpx' is not used in package 'pkg'.x",
    ] {
        let text = format!("External Dependencies\n{line}\n\nrest\n");
        assert_eq!(
            TachDiagnostic::external(&text).as_ref().map(Vec::len),
            Err(&line.to_owned()),
            "{line}"
        );
    }
    let first = "[FAIL] a.py:1: First unknown.";
    let text = format!("[FAIL] a.py:1: Ignore directive is unused.\n{first}\n[FAIL] a.py:2: Second unknown.\n");
    assert_eq!(
        TachDiagnostic::external(&text).as_ref().map(Vec::len),
        Err(&first.to_owned())
    );
}

#[test]
fn boundary_external_text_that_is_not_a_diagnostic_line_is_ignored() -> Outcome {
    let text = "General\nsome words\n[INFO] x\n  [FAIL] a.py:1: Ignore directive is unused.\n✅ All external dependencies validated!\n";
    assert_eq!(TachDiagnostic::external(text)?, []);
    assert_eq!(TachDiagnostic::external("")?, []);
    Ok(())
}

#[test]
fn boundary_external_text_missing_reason_directive_is_read_too() -> Result<(), Box<dyn std::error::Error>> {
    let found = TachDiagnostic::external("[FAIL] a.py:4: Ignore directive is missing a reason.\n")?;
    assert_eq!(found.len(), 1);
    assert_eq!(kind(&found[0]).name, "MissingIgnoreDirectiveReason");
    assert_eq!(located(&found[0]).map(|at| at.line_number), Some(4));
    Ok(())
}

#[test]
fn boundary_command_of_each_kind_is_the_one_that_reports_it() {
    let check = [
        "UndeclaredDependency",
        "ForbiddenDependency",
        "DeprecatedDependency",
        "LayerViolation",
        "ClosedLayerViolation",
        "PrivateDependency",
        "InvalidDataTypeExport",
        "UnusedIgnoreDirective",
        "MissingIgnoreDirectiveReason",
        "UnnecessarilyIgnoredDependency",
        "CircularDependency",
        "Visibility",
    ];
    let external = [
        "UndeclaredExternalDependency",
        "UnusedExternalDependency",
        "ModuleUndeclaredExternalDependency",
        "ModuleForbiddenExternalDependency",
    ];
    for name in check {
        assert_eq!(TachCommand::of(name), Some(TachCommand::Check), "{name}");
    }
    for name in external {
        assert_eq!(TachCommand::of(name), Some(TachCommand::External), "{name}");
    }
    for name in [
        "",
        "undeclareddependency",
        "NoFirstPartyImportsFound",
        "Nope",
        "Circular dependency",
    ] {
        assert_eq!(TachCommand::of(name), None, "{name}");
    }
}

#[test]
fn boundary_kind_line_renders_every_code_kind_and_none_for_one_unknown() {
    let kind_of = |name: &str, fields: &[(&str, &str)]| TachKind {
        name: name.to_owned(),
        fields: names(fields),
    };
    let table = [
        (
            kind_of(
                "UndeclaredDependency",
                &[("usage_module", "pkg.core"), ("dependency", "pkg.ui.view")],
            ),
            "pkg.core imports pkg.ui.view, undeclared",
        ),
        (
            kind_of("ForbiddenDependency", &[("usage_module", "a"), ("dependency", "b")]),
            "a imports b, forbidden",
        ),
        (
            kind_of(
                "ClosedLayerViolation",
                &[
                    ("usage_module", "a"),
                    ("usage_layer", "ui"),
                    ("dependency", "b"),
                    ("definition_layer", "low"),
                    ("closed_layer", "core"),
                ],
            ),
            "a (ui) imports b (low) across the closed layer core",
        ),
        (
            kind_of("UnusedIgnoreDirective", &[]),
            "a tach-ignore that ignores nothing",
        ),
        (
            kind_of("MissingIgnoreDirectiveReason", &[]),
            "a tach-ignore with no reason",
        ),
        (
            kind_of(
                "UnusedExternalDependency",
                &[("package_module_name", "httpx"), ("package_name", "pkg")],
            ),
            "httpx is declared in pkg, never imported",
        ),
        (
            kind_of("UndeclaredDependency", &[("usage_module", "a")]),
            "a imports dependency, undeclared",
        ),
    ];
    for (kind, expected) in table {
        assert_eq!(kind.line().as_deref(), Some(expected), "{}", kind.name);
        assert!(!kind.configuration(), "{}", kind.name);
    }
    assert_eq!(kind_of("BrandNewKind", &[("a", "b")]).line(), None);
}

#[test]
fn boundary_kind_configuration_is_the_nine_names_e_law_knows() {
    for name in [
        "ModuleConfigNotFound",
        "ModuleNotFound",
        "NoChecksEnabled",
        "NoFirstPartyImportsFound",
        "SkippedFileIoError",
        "SkippedFileSyntaxError",
        "SkippedPyProjectParsingError",
        "SkippedUnknownError",
        "UnknownLayer",
    ] {
        let kind = TachKind {
            name: name.to_owned(),
            fields: BTreeMap::new(),
        };
        assert!(kind.configuration(), "{name}");
        assert_eq!(kind.line().as_deref(), Some(name), "{name}");
    }
    let unknown = TachKind {
        name: "Other".to_owned(),
        fields: BTreeMap::new(),
    };
    assert!(!unknown.configuration());
}
