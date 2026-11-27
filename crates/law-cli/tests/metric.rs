mod common;

use common::{Out, Outcome, Project};

type Fault = Box<dyn std::error::Error>;

const MARKER: &str = " t.m m: ";
const MODULE: &str = "app/pkg/u/m.py";
const HOME: &str = "- kind: unit\n  home: [\"app/pkg/u/*.py\"]\n  suffix: Unit\n";
const BARE: &str = "- kind: unit\n  home: [\"app/pkg/u/*.py\"]\n";

fn profile(thresholds: &str) -> String {
    format!("{HOME}  thresholds: {thresholds}\n")
}

fn rule(measure: &str, over: &str, scope: &str) -> String {
    format!(
        "- id: t.m\n  message: m\n  scope: {scope}\n  check: {{engine: metric, kinds: [unit], measure: {measure}, over: {over}}}\n  fix: {{safety: none, reason: x}}\n"
    )
}

fn world(profiles: &str, rules: &str) -> Result<Project, Fault> {
    let project = Project::with_rules("[\"app\"]", rules)?;
    project.file("cat/profiles/p.yml", profiles)?;
    Ok(project)
}

fn seen(out: &Out) -> Vec<String> {
    out.stdout
        .lines()
        .filter_map(|line| {
            let (place, detail) = line.split_once(MARKER)?;
            Some(format!("{place} {detail}"))
        })
        .collect()
}

fn measured(measure: &str, limit: u32, code: &str) -> Result<Vec<String>, Fault> {
    let project = world(&profile(&format!("{{size: {limit}}}")), &rule(measure, "size", "{}"))?;
    project.file(MODULE, code)?;
    Ok(seen(&project.check(&[])?))
}

fn at(detail: &str) -> String {
    format!("{MODULE}:1:1: {detail}")
}

///// lines /////

const SOURCE: &str = "\"\"\"Doc.\n\n# not a comment: inside a docstring\nlast\n\"\"\"\n# whole-line comment\n\n    # an indented comment\nx = 1  # trailing\n\ndef f():\n    return 2\n";

#[test]
fn metric_lines_counts_code_and_docstring_lines_never_blank_or_comment_only_lines() -> Outcome {
    assert_eq!(measured("lines", 0, SOURCE)?, [at("7 lines, over 0")]);
    Ok(())
}

#[test]
fn metric_lines_a_hash_inside_a_string_is_code_not_a_comment() -> Outcome {
    let code = "x = (\n    \"a\"\n    # a comment between the parts\n    \"#b\"\n)\n";
    assert_eq!(measured("lines", 0, code)?, [at("4 lines, over 0")]);
    Ok(())
}

#[test]
fn metric_lines_a_file_without_a_final_newline_and_crlf_count_alike() -> Outcome {
    assert_eq!(measured("lines", 0, "a = 1\r\n\r\nb = 2")?, [at("2 lines, over 0")]);
    Ok(())
}

#[test]
fn metric_the_edge_equal_is_not_over_and_one_more_is() -> Outcome {
    let code = "a = 1\nb = 2\nc = 3\n";
    assert_eq!(measured("lines", 3, code)?, Vec::<String>::new());
    assert_eq!(measured("lines", 2, code)?, [at("3 lines, over 2")]);
    Ok(())
}

#[test]
fn metric_an_empty_module_measures_zero_and_is_never_over() -> Outcome {
    assert_eq!(measured("lines", 0, "")?, Vec::<String>::new());
    Ok(())
}

///// classes, companions, functions, methods /////

#[test]
fn metric_classes_counts_top_level_classes_only() -> Outcome {
    let code = "class AUnit:\n    class Inner: ...\n\n\nclass Row: ...\n\n\ndef f():\n    class Local: ...\n";
    assert_eq!(measured("classes", 1, code)?, [at("2 classes, over 1")]);
    assert_eq!(measured("classes", 2, code)?, Vec::<String>::new());
    Ok(())
}

#[test]
fn metric_companions_counts_top_level_classes_not_ending_in_the_suffix() -> Outcome {
    let code = "class AUnit: ...\n\n\nclass Row: ...\n\n\nclass Kind: ...\n\n\nclass BUnit: ...\n";
    assert_eq!(measured("companions", 1, code)?, [at("2 companions, over 1")]);
    assert_eq!(measured("companions", 2, code)?, Vec::<String>::new());
    Ok(())
}

#[test]
fn metric_functions_counts_top_level_definitions_not_methods_nor_nested_ones() -> Outcome {
    let code = "def a(): ...\n\n\nasync def b():\n    def inner(): ...\n\n\n@decorated\ndef c(): ...\n\n\nclass K:\n    def m(self): ...\n";
    assert_eq!(measured("functions", 2, code)?, [at("3 functions, over 2")]);
    Ok(())
}

#[test]
fn metric_functions_counts_every_definition_of_an_overloaded_name() -> Outcome {
    let code = "@overload\ndef f(a: int): ...\n\n\n@overload\ndef f(a: str): ...\n\n\ndef f(a): ...\n";
    assert_eq!(measured("functions", 2, code)?, [at("3 functions, over 2")]);
    Ok(())
}

#[test]
fn metric_methods_is_the_largest_class_not_the_sum() -> Outcome {
    let code = "class A:\n    def a(self): ...\n    async def b(self): ...\n\n\nclass B:\n    @property\n    def a(self): ...\n    def b(self): ...\n    def c(self): ...\n    x = 1\n";
    assert_eq!(measured("methods", 2, code)?, [at("3 methods, over 2")]);
    assert_eq!(measured("methods", 3, code)?, Vec::<String>::new());
    Ok(())
}

///// scope, homes, thresholds /////

#[test]
fn metric_a_module_outside_every_home_or_outside_the_rules_scope_is_not_measured() -> Outcome {
    let project = world(
        &profile("{size: 0}"),
        &rule("lines", "size", "{exclude: [\"**/skipped.py\"]}"),
    )?;
    project.file("app/pkg/u/skipped.py", "a = 1\n")?;
    project.file("app/pkg/u/deep/m.py", "a = 1\n")?;
    project.file("app/pkg/other.py", "a = 1\n")?;
    let out = project.check(&["-v"])?;
    assert_eq!(seen(&out), Vec::<String>::new(), "{out:?}");
    assert!(out.stderr.contains("metric: 0 modules measured\n"), "{}", out.stderr);
    Ok(())
}

#[test]
fn metric_verbose_counts_the_modules_measured_and_only_when_a_metric_rule_runs() -> Outcome {
    let project = world(&profile("{size: 100}"), &rule("lines", "size", "{}"))?;
    project.file(MODULE, "a = 1\n")?;
    project.file("app/pkg/u/n.py", "b = 2\n")?;
    project.file("app/pkg/z.py", "c = 3\n")?;
    let out = project.check(&["-v"])?;
    assert!(out.stderr.contains("metric: 2 modules measured\n"), "{}", out.stderr);
    let quiet = project.check(&[])?;
    assert!(!quiet.stderr.contains("metric:"), "{}", quiet.stderr);
    let none = world(&profile("{size: 1}"), &common::ELIF.replace("src/", "app/"))?;
    none.file(MODULE, "a = 1\n")?;
    assert!(!none.check(&["-v"])?.stderr.contains("metric:"));
    Ok(())
}

#[test]
fn metric_the_project_overrides_the_threshold_in_law_toml() -> Outcome {
    let project = world(&profile("{size: 1}"), &rule("lines", "size", "{}"))?;
    project.file(MODULE, "a = 1\nb = 2\nc = 3\n")?;
    assert_eq!(seen(&project.check(&[])?), [at("3 lines, over 1")]);
    project.config("roots = [\"app\"]\ncatalogues = [\"cat/\"]\n[profiles.unit.thresholds]\nsize = 3\n")?;
    assert_eq!(seen(&project.check(&[])?), Vec::<String>::new());
    project.config("roots = [\"app\"]\ncatalogues = [\"cat/\"]\n[profiles.unit.thresholds]\nsize = 2\n")?;
    assert_eq!(seen(&project.check(&[])?), [at("3 lines, over 2")]);
    Ok(())
}

#[test]
fn metric_a_threshold_the_profile_does_not_declare_is_a_load_fault() -> Outcome {
    let project = world(&profile("{size: 1}"), &rule("lines", "ghost", "{}"))?;
    project.file(MODULE, "a = 1\n")?;
    let out = project.check(&[])?;
    assert_eq!(
        (out.stdout.as_str(), out.stderr.as_str(), out.code),
        (
            "",
            "cat/rules/r.yml: rule `t.m` measures `lines`, which profile `unit` gives no threshold `ghost` for\n",
            2
        )
    );
    Ok(())
}

#[test]
fn metric_companions_over_a_profile_without_a_suffix_is_a_load_fault() -> Outcome {
    let profiles = format!("{BARE}  thresholds: {{size: 1}}\n");
    let project = world(&profiles, &rule("companions", "size", "{}"))?;
    let out = project.check(&[])?;
    assert_eq!(
        (out.stdout.as_str(), out.stderr.as_str(), out.code),
        (
            "",
            "cat/rules/r.yml: rule `t.m` measures `companions`, which profile `unit` gives no suffix for\n",
            2
        )
    );
    let lines = world(&profiles, &rule("lines", "size", "{}"))?;
    lines.file(MODULE, "a = 1\n")?;
    assert_eq!(lines.check(&[])?.code, 0);
    Ok(())
}

#[test]
fn metric_a_kind_no_profile_declares_and_an_unknown_measure_are_refused() -> Outcome {
    let ghost = rule("lines", "size", "{}").replace("[unit]", "[ghost]");
    let out = world(&profile("{size: 1}"), &ghost)?.check(&[])?;
    assert!(
        out.stderr.contains("no profile declares kind `ghost`"),
        "{}",
        out.stderr
    );
    assert_eq!(out.code, 2);
    let unknown = world(&profile("{size: 1}"), &rule("depth", "size", "{}"))?.check(&[])?;
    assert_eq!(unknown.code, 2, "{unknown:?}");
    let empty = rule("lines", "size", "{}").replace("[unit]", "[]");
    assert_eq!(world(&profile("{size: 1}"), &empty)?.check(&[])?.code, 2);
    Ok(())
}

#[test]
fn metric_a_threshold_is_a_real_name_never_a_number_or_an_empty_one() -> Outcome {
    for over in ["\"\"", "{}"] {
        let out = world(&profile("{size: 1}"), &rule("lines", over, "{}"))?.check(&[])?;
        assert_eq!(out.code, 2, "{over}: {out:?}");
    }
    Ok(())
}

#[test]
fn metric_two_kinds_are_judged_each_by_its_own_profile() -> Outcome {
    let profiles = format!(
        "{}- kind: part\n  home: [\"app/pkg/p/*.py\"]\n  thresholds: {{size: 3}}\n",
        profile("{size: 1}")
    );
    let rules = rule("lines", "size", "{}").replace("[unit]", "[unit, part]");
    let project = world(&profiles, &rules)?;
    project.file(MODULE, "a = 1\nb = 2\n")?;
    project.file("app/pkg/p/m.py", "a = 1\nb = 2\n")?;
    assert_eq!(seen(&project.check(&[])?), [at("2 lines, over 1")]);
    Ok(())
}

///// rule and test /////

#[test]
fn metric_rule_renders_its_engine_kinds_measure_and_threshold() -> Outcome {
    let project = world(
        &profile("{size: 1}"),
        &rule("lines", "size", "{}").replace("[unit]", "[unit, part]"),
    )?;
    project.file(
        "cat/profiles/q.yml",
        "- kind: part\n  home: [\"app/pkg/p/*.py\"]\n  thresholds: {size: 3}\n",
    )?;
    let text = project.run(&["rule", "t.m"])?;
    assert!(
        text.stdout.contains("engine: metric unit, part lines over size\n"),
        "{}",
        text.stdout
    );
    let json = project.run(&["rule", "t.m", "--output-format", "json"])?;
    let value: serde_json::Value = serde_json::from_str(&json.stdout)?;
    assert_eq!(
        value["check"],
        serde_json::json!({"engine": "metric", "kinds": ["unit", "part"], "measure": "lines", "over": "size"})
    );
    Ok(())
}

const CASES: &str = "  cases:\n    valid:\n      - path: app/pkg/u/m.py\n        code: \"a = 1\\n\"\n    invalid:\n      - path: app/pkg/u/m.py\n        code: \"a = 1\\nb = 2\\nc = 3\\n\"\n";

fn proven(rules: &str) -> Result<Out, Fault> {
    world(&profile("{size: 2}"), rules)?.run(&["test"])
}

#[test]
fn metric_test_cases_are_judged_against_the_catalogues_threshold() -> Outcome {
    let out = proven(&format!("{}{CASES}", rule("lines", "size", "{}")))?;
    assert_eq!(
        (out.stdout.as_str(), out.code),
        ("Tested 1 rule, 2 cases: 0 failed, 0 untested.\n", 0),
        "{out:?}"
    );
    Ok(())
}

#[test]
fn metric_test_a_law_toml_override_does_not_move_the_cases() -> Outcome {
    let project = world(
        &profile("{size: 2}"),
        &format!("{}{CASES}", rule("lines", "size", "{}")),
    )?;
    project.config("roots = [\"app\"]\ncatalogues = [\"cat/\"]\n[profiles.unit.thresholds]\nsize = 100\n")?;
    let out = project.run(&["test"])?;
    assert_eq!(out.code, 0, "{out:?}");
    Ok(())
}

#[test]
fn metric_test_swapped_cases_each_fail_for_their_own_reason() -> Outcome {
    let swapped = CASES
        .replace("a = 1\\n\"\n    invalid", "TMP")
        .replace("a = 1\\nb = 2\\nc = 3\\n", "a = 1\\n")
        .replace("TMP", "a = 1\\nb = 2\\nc = 3\\n\"\n    invalid");
    let out = proven(&format!("{}{swapped}", rule("lines", "size", "{}")))?;
    assert_eq!(out.code, 1, "{out:?}");
    assert!(out.stdout.contains("expected no finding"), "{}", out.stdout);
    assert!(out.stdout.contains("expected a finding, found none"), "{}", out.stdout);
    Ok(())
}

#[test]
fn metric_test_cases_all_outside_the_homes_are_an_unproven_rule() -> Outcome {
    let cases = CASES.replace("app/pkg/u/m.py", "app/pkg/elsewhere.py");
    let out = proven(&format!("{}{cases}", rule("lines", "size", "{}")))?;
    assert_eq!(out.code, 1, "{out:?}");
    assert!(
        out.stdout.contains("every case sits outside the homes of its kinds"),
        "{}",
        out.stdout
    );
    Ok(())
}
