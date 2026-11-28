mod common;

use common::{Outcome, Project, found, passed};

const MODULE: &str = "src/pkg/driver/psqlpy.py";

fn project(spec: &str) -> Result<Project, Box<dyn std::error::Error>> {
    Project::with_rules(
        "[\"src\"]",
        &format!(
            "- id: t.shape\n  message: shape holds\n  scope: {{include: [\"src/**/*.py\"]}}\n  check: {{engine: shape, {spec}}}\n  fix: {{safety: none, reason: x}}\n"
        ),
    )
}

fn verdict(spec: &str, code: &str) -> Result<common::Out, Box<dyn std::error::Error>> {
    let project = project(spec)?;
    project.file(MODULE, code)?;
    project.check(&[])
}

fn at(line: u32, column: u32, detail: &str) -> String {
    found(
        &format!("{MODULE}:{line}:{column}"),
        "t.shape",
        &format!("shape holds: {detail}"),
        1,
        0,
    )
}

#[test]
fn shape_classes_count_fails_at_line_1() -> Outcome {
    let out = verdict("classes: {count: 1}", "class A: ...\nclass B: ...\n")?;
    assert_eq!(out.stdout, at(1, 1, "2 classes, 1 allowed"));
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn shape_classes_count_zero_found_fails_and_exact_passes() -> Outcome {
    let none = verdict("classes: {count: 1}", "x = 1\n")?;
    assert_eq!(none.stdout, at(1, 1, "0 classes, 1 allowed"));
    assert!(passed(&verdict("classes: {count: 1}", "class A: ...\n")?));
    Ok(())
}

#[test]
fn shape_classes_name_template_fills_dir_and_stem() -> Outcome {
    let spec = "classes: {name: \"{Stem}{Dir}\"}";
    assert!(passed(&verdict(spec, "class PsqlpyDriver: ...\n")?));
    let out = verdict(spec, "x = 1\n\nclass Other: ...\n")?;
    assert_eq!(out.stdout, at(3, 1, "class Other is not named PsqlpyDriver"));
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn shape_classes_name_decorated_class_is_judged_at_its_definition() -> Outcome {
    let out = verdict("classes: {name: \"{Stem}{Dir}\"}", "@frozen\nclass Other: ...\n")?;
    assert_eq!(out.stdout, at(2, 1, "class Other is not named PsqlpyDriver"));
    Ok(())
}

#[test]
fn shape_classes_bases_glob_matches_the_bare_name_of_one_base() -> Outcome {
    let spec = "classes: {bases: \"*Base\"}";
    assert!(passed(&verdict(
        spec,
        "class A(mod.DriverBase[T], metaclass=Meta): ...\n"
    )?));
    assert!(passed(&verdict(spec, "class A(Generic[T], DriverBase): ...\n")?));
    let out = verdict(spec, "class A(Other): ...\n")?;
    assert_eq!(out.stdout, at(1, 1, "class A has no base named *Base"));
    let bare = verdict(spec, "class B: ...\n")?;
    assert_eq!(bare.stdout, at(1, 1, "class B has no base named *Base"));
    Ok(())
}

#[test]
fn shape_functions_max_flags_each_beyond_the_allowed() -> Outcome {
    let out = verdict("functions: {max: 1}", "def a(): ...\ndef b(): ...\ndef c(): ...\n")?;
    assert_eq!(
        out.stdout,
        "src/pkg/driver/psqlpy.py:2:1: t.shape shape holds: function b beyond the 1 allowed\nsrc/pkg/driver/psqlpy.py:3:1: t.shape shape holds: function c beyond the 1 allowed\n\nFound 2 findings (2 errors, 0 warnings).\n"
    );
    assert!(passed(&verdict("functions: {max: 1}", "def a(): ...\n")?));
    Ok(())
}

#[test]
fn shape_functions_asynchronous_forbids_sync() -> Outcome {
    let spec = "functions: {asynchronous: true}";
    assert!(passed(&verdict(
        spec,
        "async def a(): ...\n@get('/')\nasync def b(): ...\n"
    )?));
    let out = verdict(spec, "async def a(): ...\ndef b(): ...\n")?;
    assert_eq!(out.stdout, at(2, 1, "function b is not async"));
    let decorated = verdict(spec, "@cache\ndef c(): ...\n")?;
    assert_eq!(decorated.stdout, at(2, 1, "function c is not async"));
    Ok(())
}

#[test]
fn shape_constants_max_flags_each_beyond_the_allowed() -> Outcome {
    let out = verdict("constants: {max: 1}", "A = 1\nB = 2\n")?;
    assert_eq!(out.stdout, at(2, 1, "constant beyond the 1 allowed"));
    assert!(passed(&verdict("constants: {max: 1}", "A = 1\n")?));
    Ok(())
}

#[test]
fn shape_constants_annotation_must_match_the_whole_text() -> Outcome {
    let spec = "constants: {annotation: \"Final(\\\\[.*\\\\])?\"}";
    assert!(passed(&verdict(spec, "A: Final = 1\nB: Final[int] = 2\n")?));
    let out = verdict(spec, "A: int = 1\nB = 2\n")?;
    assert_eq!(
        out.stdout,
        "src/pkg/driver/psqlpy.py:1:1: t.shape shape holds: annotation int is not Final(\\[.*\\])?\nsrc/pkg/driver/psqlpy.py:2:1: t.shape shape holds: annotation none is not Final(\\[.*\\])?\n\nFound 2 findings (2 errors, 0 warnings).\n"
    );
    Ok(())
}

#[test]
fn shape_constants_literals_false_forbids_a_string_in_the_value() -> Outcome {
    let spec = "constants: {literals: false}";
    assert!(passed(&verdict(spec, "A = 1\nB = b'raw'\nC = f'{A}'\nD = (1, 2)\n")?));
    let out = verdict(spec, "A = 'x'\nB = (1, \"y\")\n")?;
    assert_eq!(
        out.stdout,
        "src/pkg/driver/psqlpy.py:1:1: t.shape shape holds: a string literal in a constant\nsrc/pkg/driver/psqlpy.py:2:1: t.shape shape holds: a string literal in a constant\n\nFound 2 findings (2 errors, 0 warnings).\n"
    );
    Ok(())
}

#[test]
fn shape_constants_literals_allowed_by_default() -> Outcome {
    assert!(passed(&verdict("constants: {max: 3}", "A = 'x'\n")?));
    Ok(())
}

#[test]
fn shape_judges_only_the_top_level() -> Outcome {
    let code = "class A:\n    X = 1\n    def f(self): ...\n    class B: ...\n";
    assert!(passed(&verdict(
        "classes: {count: 1}, functions: {max: 0}, constants: {max: 0}",
        code
    )?));
    Ok(())
}

#[test]
fn shape_scope_fences_the_modules_judged() -> Outcome {
    let project = project("classes: {count: 0}")?;
    project.file("src/a.py", "")?;
    project.file("tests/a.py", "class A: ...\n")?;
    project.config("roots = [\"src\", \"tests\"]\ncatalogues = [\"cat/\"]\n")?;
    assert!(passed(&project.check(&[])?));
    Ok(())
}

#[test]
fn shape_order_and_sections_left_the_model_as_unknown_fields() -> Outcome {
    for (spec, field) in [
        ("order: privates-first", "order"),
        ("sections: \"#+ .+ #+\"", "sections"),
        ("classes: {count: 1}, order: privates-first", "order"),
    ] {
        let project = project(spec)?;
        project.file(MODULE, "class A: ...\n")?;
        let out = project.check(&[])?;
        assert_eq!(out.code, 2, "{spec}");
        assert_eq!(out.stdout, "", "{spec}");
        assert!(
            out.stderr.starts_with(&format!(
                "cat/rules/r.yml:4: unknown field `{field}`, expected one of classes, functions, constants"
            )),
            "{spec}: {}",
            out.stderr
        );
    }
    Ok(())
}

#[test]
fn shape_load_refuses_an_unknown_placeholder_and_an_empty_template() -> Outcome {
    for name in ["{Nope}Driver", ""] {
        let out = project(&format!("classes: {{name: \"{name}\"}}"))?.check(&[])?;
        assert_eq!((out.stdout.as_str(), out.code), ("", 2), "{name}: {out:?}");
        assert!(out.stderr.contains("bad-template"), "{name}: {}", out.stderr);
    }
    Ok(())
}

#[test]
fn shape_classes_name_template_keeps_its_glob_semantics() -> Outcome {
    let spec = "classes: {name: \"{Stem}*\"}";
    assert!(passed(&verdict(
        spec,
        "class PsqlpyDriver: ...\nclass PsqlpyOther: ...\n"
    )?));
    Ok(())
}
