use law_model::{Check, FixPolicy, Rule, Severity, Snippet};
use serde_json::{Value, json};

type Outcome = Result<(), Box<dyn std::error::Error>>;

const SAMPLE: &str = include_str!("fixtures/rule.yml");
const MINIMAL: &str = include_str!("fixtures/rule_minimal.yml");

fn sample() -> Result<Rule, Box<dyn std::error::Error>> {
    serde_saphyr::from_str::<Vec<Rule>>(SAMPLE)?
        .into_iter()
        .next()
        .ok_or_else(|| "empty sample".into())
}

fn base() -> Value {
    json!({
        "id": "lexis.elif",
        "message": "one line",
        "check": {"engine": "pattern", "rule": {"kind": "elif_clause"}},
        "fix": {"safety": "safe"},
    })
}

fn with(key: &str, value: Value) -> Value {
    let mut rule = base();
    rule[key] = value;
    rule
}

fn without(key: &str) -> Value {
    let mut rule = base();
    if let Some(map) = rule.as_object_mut() {
        map.remove(key);
    }
    rule
}

fn refused(rule: Value) -> bool {
    serde_json::from_value::<Rule>(rule).is_err()
}

#[test]
fn rule_sample_loads_every_field() -> Outcome {
    let rule = sample()?;
    assert_eq!(rule.id.as_str(), "lexis.elif");
    assert_eq!(rule.message.as_str(), "elif is a decision table nobody wrote down");
    assert!(rule.tags.contains("law", "5"));
    assert!(rule.tags.contains("set", "LEXIS"));
    assert!(rule.tags.contains("principle", "dispatch"));
    assert_eq!(rule.scope.include.len(), 1);
    assert_eq!(rule.scope.exclude.len(), 1);
    assert_eq!(rule.scope.kind.as_ref().map(law_model::Kind::as_str), Some("engine"));
    assert_eq!(rule.severity, Severity::Error);
    assert!(matches!(rule.check, Check::Pattern(_)));
    assert_eq!(
        rule.fix,
        FixPolicy::None {
            reason: "the table has to be designed".to_owned()
        }
    );
    assert_eq!(rule.cases.valid.len(), 1);
    assert_eq!(rule.cases.invalid.len(), 1);
    assert!(rule.cases.fixed.is_empty());
    Ok(())
}

#[test]
fn rule_sample_case_path_and_code() -> Outcome {
    let rule = sample()?;
    let invalid = rule.cases.invalid.first().ok_or("no invalid case")?;
    assert_eq!(
        invalid.path.as_deref(),
        Some(std::path::Path::new("src/pkg/logic/x.py"))
    );
    assert_eq!(
        invalid.snippet,
        Snippet::Code("if a:\n  pass\nelif b:\n  pass\n".to_owned())
    );
    let valid = rule.cases.valid.first().ok_or("no valid case")?;
    assert!(valid.path.is_none());
    Ok(())
}

#[test]
fn rule_sample_round_trips_through_yaml() -> Outcome {
    let rule = sample()?;
    let yaml = serde_saphyr::to_string(&vec![rule.clone()])?;
    let back: Vec<Rule> = serde_saphyr::from_str(&yaml)?;
    assert_eq!(back, vec![rule]);
    Ok(())
}

#[test]
fn rule_sample_round_trips_through_json() -> Outcome {
    let rule = sample()?;
    let back: Rule = serde_json::from_str(&serde_json::to_string(&rule)?)?;
    assert_eq!(back, rule);
    Ok(())
}

#[test]
fn rule_minimal_takes_defaults() -> Outcome {
    let rule = serde_saphyr::from_str::<Vec<Rule>>(MINIMAL)?
        .into_iter()
        .next()
        .ok_or("empty")?;
    assert_eq!(
        rule.scope
            .include
            .iter()
            .map(law_model::Glob::as_str)
            .collect::<Vec<_>>(),
        ["**/*.py"]
    );
    assert!(rule.scope.exclude.is_empty());
    assert_eq!(rule.severity, Severity::Error);
    assert_eq!(rule.fix, FixPolicy::Safe);
    assert!(rule.cases.valid.is_empty() && rule.cases.invalid.is_empty());
    assert!(matches!(rule.check, Check::Tree(_)));
    Ok(())
}

#[test]
fn rule_bare_string_case_is_code_only() -> Outcome {
    let rule: Rule = serde_json::from_value(with("cases", json!({"valid": ["x = 1\n"], "invalid": ["y = 2\n"]})))?;
    let valid = rule.cases.valid.first().ok_or("no case")?;
    assert_eq!(valid.snippet, Snippet::Code("x = 1\n".to_owned()));
    assert!(valid.path.is_none());
    Ok(())
}

#[test]
fn rule_fixed_case_requires_expect() -> Outcome {
    assert!(serde_json::from_value::<Rule>(with("cases", json!({"fixed": [{"code": "a"}]}))).is_err());
    let rule: Rule = serde_json::from_value(with("cases", json!({"fixed": [{"code": "a", "expect": "b"}]})))?;
    assert_eq!(rule.cases.fixed.len(), 1);
    Ok(())
}

#[test]
fn rule_refuses_bad_ids() {
    for id in ["Elif", "lexis", "lexis."] {
        assert!(refused(with("id", json!(id))), "{id:?}");
    }
}

#[test]
fn rule_refuses_multiline_message() {
    assert!(refused(with("message", json!("a\nb"))));
}

#[test]
fn rule_refuses_missing_fix() {
    assert!(refused(without("fix")));
}

#[test]
fn rule_refuses_none_without_reason() {
    assert!(refused(with("fix", json!({"safety": "none"}))));
}

#[test]
fn rule_refuses_unknown_safety() {
    assert!(refused(with("fix", json!({"safety": "maybe"}))));
}

#[test]
fn rule_accepts_none_with_reason() -> Outcome {
    let rule: Rule = serde_json::from_value(with("fix", json!({"safety": "none", "reason": "design"})))?;
    assert_eq!(
        rule.fix,
        FixPolicy::None {
            reason: "design".to_owned()
        }
    );
    Ok(())
}

#[test]
fn rule_refuses_unknown_engine() {
    assert!(refused(with("check", json!({"engine": "magic", "rule": {}}))));
}

#[test]
fn rule_refuses_missing_engine_tag() {
    assert!(refused(with("check", json!({"rule": {"kind": "x"}}))));
}

#[test]
fn rule_refuses_missing_check() {
    assert!(refused(without("check")));
}

#[test]
fn rule_refuses_unknown_severity() {
    assert!(refused(with("severity", json!("fatal"))));
}

#[test]
fn rule_refuses_unknown_field_at_top() {
    assert!(refused(with("extra", json!(1))));
}

#[test]
fn rule_refuses_unknown_field_in_scope() {
    assert!(refused(with("scope", json!({"include": ["a/*.py"], "extra": 1}))));
}

#[test]
fn rule_refuses_unknown_field_in_cases() {
    assert!(refused(with("cases", json!({"valid": [], "extra": []}))));
}

#[test]
fn rule_refuses_unknown_field_in_placed_case() {
    assert!(refused(with("cases", json!({"valid": [{"code": "x", "extra": 1}]}))));
}

#[test]
fn rule_refuses_unknown_field_in_fixed_case() {
    assert!(refused(with(
        "cases",
        json!({"fixed": [{"code": "x", "expect": "y", "extra": 1}]})
    )));
}

#[test]
fn rule_refuses_unknown_field_in_fix_policy() {
    assert!(refused(with("fix", json!({"safety": "safe", "extra": 1}))));
}

#[test]
fn rule_refuses_unknown_key_in_pattern_check() {
    assert!(refused(with(
        "check",
        json!({"engine": "pattern", "rule": {"kind": "x"}, "extra": 1})
    )));
}

#[test]
fn rule_refuses_unknown_key_in_tree_check() {
    assert!(refused(with(
        "check",
        json!({"engine": "tree", "dirs": ["a/**"], "extra": 1})
    )));
}

#[test]
fn rule_refuses_unknown_key_in_shape_check() {
    assert!(refused(with("check", json!({"engine": "shape", "extra": 1}))));
}

#[test]
fn rule_refuses_unknown_key_in_shape_classes() {
    assert!(refused(with(
        "check",
        json!({"engine": "shape", "classes": {"count": 1, "extra": 1}})
    )));
}

#[test]
fn rule_refuses_unknown_key_in_shape_functions() {
    assert!(refused(with(
        "check",
        json!({"engine": "shape", "functions": {"max": 1, "extra": 1}})
    )));
}

#[test]
fn rule_refuses_unknown_key_in_shape_constants() {
    assert!(refused(with(
        "check",
        json!({"engine": "shape", "constants": {"max": 1, "extra": 1}})
    )));
}

#[test]
fn rule_accepts_every_native_check() -> Outcome {
    for check in [
        json!({"engine": "tree", "dirs": ["a/**"], "allow": ["*.py", "x/"], "require": ["__x.py"], "name": "[a-z]+"}),
        json!({"engine": "shape",
               "classes": {"count": 1, "name": "{Stem}{Dir}", "bases": "*Base"},
               "functions": {"max": 0, "asynchronous": true},
               "constants": {"max": 2, "annotation": "Final.*", "literals": false}}),
        json!({"engine": "members", "lineage": {"common": ["common"]}, "order": "privates-first", "hooks": ["field_validator"]}),
        json!({"engine": "members", "order": "privates-first"}),
        json!({"engine": "members", "lineage": {}}),
        json!({"engine": "pattern", "rule": {"kind": "x"}, "constraints": {}, "utils": {}, "transform": {}, "fix": "y"}),
    ] {
        let rule: Rule = serde_json::from_value(with("check", check.clone()))?;
        let back: Rule = serde_json::from_str(&serde_json::to_string(&rule)?)?;
        assert_eq!(back, rule, "{check}");
    }
    Ok(())
}

#[test]
fn rule_refuses_bad_regex_and_glob_in_check() {
    assert!(refused(with("check", json!({"engine": "tree", "dirs": ["a/["]}))));
    assert!(refused(with(
        "check",
        json!({"engine": "tree", "dirs": ["a/**"], "name": "("})
    )));
    assert!(refused(with(
        "check",
        json!({"engine": "shape", "classes": {"name": "[{Dir}"}})
    )));
}

#[test]
fn rule_refuses_shape_order_and_sections_as_unknown_fields() {
    assert!(refused(with("check", json!({"engine": "shape", "sections": "#+"}))));
    assert!(refused(with(
        "check",
        json!({"engine": "shape", "order": "privates-first"})
    )));
}

#[test]
fn rule_refuses_members_with_neither_lineage_nor_order() {
    assert!(refused(with("check", json!({"engine": "members"}))));
    assert!(refused(with("check", json!({"engine": "members", "hooks": ["x"]}))));
}

#[test]
fn rule_refuses_members_unknown_order_and_unknown_fields() {
    assert!(refused(with(
        "check",
        json!({"engine": "members", "order": "publics-first"})
    )));
    assert!(refused(with(
        "check",
        json!({"engine": "members", "order": "privates-first", "extra": 1})
    )));
    assert!(refused(with(
        "check",
        json!({"engine": "members", "lineage": {"commons": []}})
    )));
}

#[test]
fn rule_refuses_tree_without_dirs() {
    assert!(refused(with("check", json!({"engine": "tree"}))));
}

#[test]
fn rule_refuses_pattern_without_rule() {
    assert!(refused(with("check", json!({"engine": "pattern"}))));
}

#[test]
fn rule_refuses_reason_on_a_fix_that_exists() {
    assert!(refused(with("fix", json!({"safety": "safe", "reason": "why"}))));
    assert!(refused(with("fix", json!({"safety": "unsafe", "reason": "why"}))));
}

#[test]
fn rule_formerly_lists_plain_words_and_is_kept_in_order() -> Outcome {
    let rule: Rule = serde_json::from_value(with("formerly", json!(["object-annotation", "any-annotation"])))?;
    assert_eq!(
        serde_json::to_value(&rule)?["formerly"],
        json!(["object-annotation", "any-annotation"])
    );
    Ok(())
}

#[test]
fn rule_formerly_absent_is_empty_and_is_not_serialised() -> Outcome {
    let rule: Rule = serde_json::from_value(base())?;
    assert!(rule.formerly.is_empty());
    assert!(serde_json::to_value(&rule)?.get("formerly").is_none());
    Ok(())
}

#[test]
fn rule_formerly_refuses_what_is_not_one_lowercase_word() {
    for bad in ["a.b", "Bad", "", "1x", "a b", "a_b"] {
        assert!(refused(with("formerly", json!([bad]))), "{bad:?}");
    }
    assert!(refused(with("formerly", json!("object-annotation"))));
}

fn ruff(settings: &Value) -> Result<Rule, serde_json::Error> {
    serde_json::from_value(with(
        "check",
        json!({"engine": "ruff", "code": "TID251", "settings": settings}),
    ))
}

#[test]
fn rule_ruff_settings_render_as_one_inline_toml_override_per_key_in_key_order() -> Outcome {
    let rule = ruff(&json!({
        "lint.flake8-tidy-imports.banned-api": {"os.environ": {"msg": "say \"no\"\n"}, "os.getenv": {"msg": "m"}},
        "line-length": 100,
        "lint.select": ["E", "F"],
        "lint.preview": true,
        "lint.ratio": 1.5,
    }))?;
    let Check::Ruff(spec) = rule.check else {
        return Err("not a ruff check".into());
    };
    assert_eq!(
        spec.overrides(),
        [
            "line-length = 100",
            "lint.flake8-tidy-imports.banned-api = {\"os.environ\" = {\"msg\" = \"say \\\"no\\\"\\n\"}, \"os.getenv\" = {\"msg\" = \"m\"}}",
            "lint.preview = true",
            "lint.ratio = 1.5",
            "lint.select = [\"E\", \"F\"]",
        ]
    );
    Ok(())
}

#[test]
fn rule_ruff_settings_absent_render_nothing_and_are_not_written_back() -> Outcome {
    let rule: Rule = serde_json::from_value(with("check", json!({"engine": "ruff", "code": "T201"})))?;
    let Check::Ruff(spec) = &rule.check else {
        return Err("not a ruff check".into());
    };
    assert!(spec.overrides().is_empty());
    assert!(serde_json::to_value(&rule)?["check"].get("settings").is_none());
    Ok(())
}

#[test]
fn rule_ruff_settings_holding_a_null_anywhere_are_refused() {
    for settings in [
        json!({"k": null}),
        json!({"k": [1, null]}),
        json!({"k": {"a": {"b": null}}}),
    ] {
        assert!(ruff(&settings).is_err(), "{settings}");
    }
}

#[test]
fn rule_ruff_settings_round_trip_through_serde() -> Outcome {
    let settings = json!({"k": {"a": [1, "x", {"b": false}]}});
    let rule = ruff(&settings)?;
    assert_eq!(serde_json::to_value(&rule)?["check"]["settings"], settings);
    assert_eq!(serde_json::from_value::<Rule>(serde_json::to_value(&rule)?)?, rule);
    Ok(())
}
