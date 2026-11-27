use law_config::{Config, Exceptions, schema};
use law_model::{Catalogue, Profile, Report, Rule, Selector};
use schemars::Schema;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

type Outcome = Result<(), Box<dyn std::error::Error>>;

const DRAFT: &str = "https://json-schema.org/draft/2020-12/schema";
const RULES: &str = include_str!("fixtures/eager/rules/lexis.yml");
const PROFILES: &str = include_str!("fixtures/eager/profiles/engine.yml");
const LAW: &str = include_str!("fixtures/law.toml");
const PYPROJECT: &str = include_str!("fixtures/pyproject.toml");

fn value(schema: &Schema) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(serde_json::to_value(schema)?)
}

fn accepts(schema: &Schema, instance: &Value) -> Result<bool, Box<dyn std::error::Error>> {
    let validator = jsonschema::draft202012::options()
        .should_validate_formats(true)
        .build(&value(schema)?)?;
    Ok(validator.is_valid(instance))
}

fn why(schema: &Schema, instance: &Value) -> Result<String, Box<dyn std::error::Error>> {
    let validator = jsonschema::draft202012::options()
        .should_validate_formats(true)
        .build(&value(schema)?)?;
    Ok(validator
        .iter_errors(instance)
        .map(|error| error.to_string())
        .collect::<Vec<_>>()
        .join("; "))
}

fn rule_json() -> Result<Value, Box<dyn std::error::Error>> {
    Ok(serde_saphyr::from_str(RULES)?)
}

fn rule_with(key: &str, replacement: Value) -> Result<Value, Box<dyn std::error::Error>> {
    let mut rules = rule_json()?;
    rules[0][key] = replacement;
    Ok(rules)
}

fn rule_without(key: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let mut rules = rule_json()?;
    if let Some(map) = rules[0].as_object_mut() {
        map.remove(key);
    }
    Ok(rules)
}

fn exception(edit: impl FnOnce(&mut serde_json::Map<String, Value>)) -> Value {
    let mut value = json!({"rule": "lexis.elif", "path": "app/legacy/**", "reason": "r", "approved": "a"});
    if let Some(map) = value.as_object_mut() {
        edit(map);
    }
    json!({"exception": [value]})
}

fn agree<T: DeserializeOwned>(
    schema: &Schema,
    cases: &[(&str, Value)],
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    cases
        .iter()
        .map(|(name, instance)| {
            let serde_refuses = serde_json::from_value::<T>(instance.clone()).is_err();
            let schema_refuses = !accepts(schema, instance)?;
            Ok((serde_refuses, schema_refuses, name))
        })
        .filter_map(|result: Result<_, Box<dyn std::error::Error>>| match result {
            Ok((true, true, _)) => None,
            Ok((serde_refuses, schema_refuses, name)) => Some(Ok(format!(
                "{name}: serde refuses={serde_refuses}, schema rejects={schema_refuses}"
            ))),
            Err(error) => Some(Err(error)),
        })
        .collect()
}

#[test]
fn schema_every_schema_is_valid_json_schema_2020_12() -> Outcome {
    let all = schema();
    for (name, schema) in [
        ("config", &all.config),
        ("rules", &all.rules),
        ("profiles", &all.profiles),
        ("exceptions", &all.exceptions),
        ("report", &all.report),
    ] {
        let json = value(schema)?;
        assert_eq!(json["$schema"], json!(DRAFT), "{name}");
        jsonschema::draft202012::meta::validate(&json).map_err(|error| format!("{name}: {error}"))?;
        jsonschema::draft202012::options()
            .should_validate_formats(true)
            .build(&json)
            .map_err(|error| format!("{name}: {error}"))?;
    }
    Ok(())
}

#[test]
fn schema_rules_accept_the_sample() -> Outcome {
    let sample = rule_json()?;
    assert!(accepts(&schema().rules, &sample)?, "{}", why(&schema().rules, &sample)?);
    Ok(())
}

#[test]
fn schema_rules_accept_minimal_and_every_native_engine() -> Outcome {
    let all = schema();
    for check in [
        json!({"engine": "tree", "dirs": ["a/**"]}),
        json!({"engine": "tree", "dirs": ["a/**"], "allow": ["*.py"], "require": ["x.py"], "name": "[a-z]+"}),
        json!({"engine": "shape"}),
        json!({"engine": "shape", "classes": {"count": 1, "name": "{Stem}", "bases": "*Base"},
               "functions": {"max": 0, "asynchronous": true},
               "constants": {"max": 1, "annotation": "Final", "literals": false}}),
        json!({"engine": "members", "order": "privates-first"}),
        json!({"engine": "members", "lineage": {"common": ["common"]}, "hooks": ["field_validator"]}),
        json!({"engine": "pattern", "rule": {"kind": "x"}, "constraints": {}, "fix": "y"}),
    ] {
        let instance = json!([{"id": "a.b", "message": "m", "check": check, "fix": {"safety": "unsafe"}}]);
        assert!(
            accepts(&all.rules, &instance)?,
            "{check}: {}",
            why(&all.rules, &instance)?
        );
        assert!(serde_json::from_value::<Vec<Rule>>(instance).is_ok(), "{check}");
    }
    Ok(())
}

#[test]
fn schema_rules_accept_bare_string_cases_and_integer_tags() -> Outcome {
    let mut rules = rule_json()?;
    rules[0]["cases"] = json!({"valid": ["x = 1\n"], "invalid": ["y = 2\n"]});
    rules[0]["tags"] = json!({"law": 5, "set": ["OST", "LEXIS"]});
    assert!(accepts(&schema().rules, &rules)?, "{}", why(&schema().rules, &rules)?);
    Ok(())
}

#[test]
fn schema_rules_reject_each_refusal_serde_refuses() -> Outcome {
    let mismatches = agree::<Vec<Rule>>(
        &schema().rules,
        &[
            ("bad id Elif", rule_with("id", json!("Elif"))?),
            ("bad id lexis", rule_with("id", json!("lexis"))?),
            ("bad id lexis.", rule_with("id", json!("lexis."))?),
            ("multiline message", rule_with("message", json!("a\nb"))?),
            ("empty message", rule_with("message", json!(""))?),
            ("missing fix", rule_without("fix")?),
            ("missing check", rule_without("check")?),
            ("none without reason", rule_with("fix", json!({"safety": "none"}))?),
            ("unknown safety", rule_with("fix", json!({"safety": "maybe"}))?),
            (
                "unknown engine",
                rule_with("check", json!({"engine": "magic", "rule": {}}))?,
            ),
            ("no engine tag", rule_with("check", json!({"rule": {"kind": "x"}}))?),
            ("unknown field", rule_with("extra", json!(1))?),
            ("unknown severity", rule_with("severity", json!("fatal"))?),
            (
                "unknown scope field",
                rule_with("scope", json!({"include": ["a"], "extra": 1}))?,
            ),
            ("unknown cases field", rule_with("cases", json!({"extra": []}))?),
            (
                "unknown case field",
                rule_with("cases", json!({"valid": [{"code": "x", "extra": 1}]}))?,
            ),
            (
                "fixed without expect",
                rule_with("cases", json!({"fixed": [{"code": "x"}]}))?,
            ),
            ("bad kind", rule_with("scope", json!({"kind": "Engine"}))?),
            ("nested tag", rule_with("tags", json!({"k": {"a": 1}}))?),
            ("not a list", rule_json()?[0].clone()),
        ],
    )?;
    assert!(mismatches.is_empty(), "{mismatches:#?}");
    Ok(())
}

#[test]
fn schema_rules_reject_unknown_key_inside_each_engine_check() -> Outcome {
    let mismatches = agree::<Vec<Rule>>(
        &schema().rules,
        &[
            (
                "pattern",
                rule_with("check", json!({"engine": "pattern", "rule": {}, "extra": 1}))?,
            ),
            (
                "tree",
                rule_with("check", json!({"engine": "tree", "dirs": [], "extra": 1}))?,
            ),
            ("shape", rule_with("check", json!({"engine": "shape", "extra": 1}))?),
            (
                "shape classes",
                rule_with("check", json!({"engine": "shape", "classes": {"extra": 1}}))?,
            ),
            (
                "shape functions",
                rule_with("check", json!({"engine": "shape", "functions": {"extra": 1}}))?,
            ),
            (
                "shape constants",
                rule_with("check", json!({"engine": "shape", "constants": {"extra": 1}}))?,
            ),
            (
                "shape order, left the model",
                rule_with("check", json!({"engine": "shape", "order": "privates-first"}))?,
            ),
            (
                "shape sections, left the model",
                rule_with("check", json!({"engine": "shape", "sections": "#+"}))?,
            ),
            (
                "members",
                rule_with(
                    "check",
                    json!({"engine": "members", "order": "privates-first", "extra": 1}),
                )?,
            ),
            (
                "members lineage",
                rule_with("check", json!({"engine": "members", "lineage": {"extra": 1}}))?,
            ),
            (
                "members unknown order",
                rule_with("check", json!({"engine": "members", "order": "publics-first"}))?,
            ),
            ("tree without dirs", rule_with("check", json!({"engine": "tree"}))?),
            (
                "pattern without rule",
                rule_with("check", json!({"engine": "pattern"}))?,
            ),
        ],
    )?;
    assert!(mismatches.is_empty(), "{mismatches:#?}");
    Ok(())
}

#[test]
fn schema_rules_reject_members_without_lineage_or_order() -> Outcome {
    let mismatches = agree::<Vec<Rule>>(
        &schema().rules,
        &[
            ("members bare", rule_with("check", json!({"engine": "members"}))?),
            (
                "members hooks only",
                rule_with("check", json!({"engine": "members", "hooks": ["x"]}))?,
            ),
        ],
    )?;
    assert!(mismatches.is_empty(), "{mismatches:#?}");
    Ok(())
}

#[test]
fn schema_rules_reject_a_reason_on_a_fix_that_exists() -> Outcome {
    let mismatches = agree::<Vec<Rule>>(
        &schema().rules,
        &[(
            "safe with reason",
            rule_with("fix", json!({"safety": "safe", "reason": "why"}))?,
        )],
    )?;
    assert!(mismatches.is_empty(), "{mismatches:#?}");
    Ok(())
}

#[test]
fn schema_profiles_accept_the_sample_and_reject_refusals() -> Outcome {
    let all = schema();
    let sample: Value = serde_saphyr::from_str(PROFILES)?;
    assert!(accepts(&all.profiles, &sample)?, "{}", why(&all.profiles, &sample)?);
    let mismatches = agree::<Vec<Profile>>(
        &all.profiles,
        &[
            ("unknown field", json!([{"kind": "engine", "home": [], "extra": 1}])),
            ("bad kind", json!([{"kind": "Engine", "home": []}])),
            ("missing home", json!([{"kind": "engine"}])),
            ("missing kind", json!([{"home": []}])),
        ],
    )?;
    assert!(mismatches.is_empty(), "{mismatches:#?}");
    Ok(())
}

#[test]
fn schema_config_accepts_the_law_toml_sample() -> Outcome {
    let sample: Value = toml::from_str(LAW)?;
    assert!(
        accepts(&schema().config, &sample)?,
        "{}",
        why(&schema().config, &sample)?
    );
    Ok(())
}

#[test]
fn schema_config_accepts_tool_law_from_pyproject() -> Outcome {
    let pyproject: Value = toml::from_str(PYPROJECT)?;
    let sample = pyproject["tool"]["law"].clone();
    assert!(
        accepts(&schema().config, &sample)?,
        "{}",
        why(&schema().config, &sample)?
    );
    Ok(())
}

#[test]
fn schema_config_accepts_the_empty_config() -> Outcome {
    assert!(accepts(&schema().config, &json!({}))?);
    Ok(())
}

#[test]
fn schema_config_rejects_each_refusal_serde_refuses() -> Outcome {
    let mismatches = agree::<Config>(
        &schema().config,
        &[
            ("unknown key", json!({"rots": ["src"]})),
            ("unknown override key", json!({"profiles": {"engine": {"lines": 3}}})),
            (
                "bad kind",
                json!({"profiles": {"Engine": {"thresholds": {"lines": 1}}}}),
            ),
            (
                "string threshold",
                json!({"profiles": {"engine": {"thresholds": {"lines": "many"}}}}),
            ),
            (
                "negative threshold",
                json!({"profiles": {"engine": {"thresholds": {"lines": -1}}}}),
            ),
            ("roots not a list", json!({"roots": "src"})),
            ("selector =5", json!({"select": ["=5"]})),
            ("selector law=", json!({"ignore": ["law="]})),
            ("selector Lexis", json!({"select": ["Lexis"]})),
            ("selector lexis", json!({"select": ["lexis"]})),
        ],
    )?;
    assert!(mismatches.is_empty(), "{mismatches:#?}");
    Ok(())
}

#[test]
fn schema_exceptions_accept_path_symbol_and_both() -> Outcome {
    let all = schema();
    for instance in [
        exception(|_| {}),
        exception(|map| {
            map.remove("path");
            map.insert("symbol".into(), json!("Legacy.run"));
        }),
        exception(|map| {
            map.insert("symbol".into(), json!("Legacy.run"));
            map.insert("expires".into(), json!("2027-03-01"));
        }),
    ] {
        assert!(
            accepts(&all.exceptions, &instance)?,
            "{instance}: {}",
            why(&all.exceptions, &instance)?
        );
        assert!(serde_json::from_value::<Exceptions>(instance).is_ok());
    }
    Ok(())
}

#[test]
fn schema_exceptions_reject_each_refusal_serde_refuses() -> Outcome {
    let mismatches = agree::<Exceptions>(
        &schema().exceptions,
        &[
            (
                "neither path nor symbol",
                exception(|map| {
                    map.remove("path");
                }),
            ),
            (
                "empty reason",
                exception(|map| {
                    map.insert("reason".into(), json!(""));
                }),
            ),
            (
                "empty approver",
                exception(|map| {
                    map.insert("approved".into(), json!(""));
                }),
            ),
            (
                "empty symbol",
                exception(|map| {
                    map.insert("symbol".into(), json!(""));
                }),
            ),
            (
                "missing reason",
                exception(|map| {
                    map.remove("reason");
                }),
            ),
            (
                "missing approver",
                exception(|map| {
                    map.remove("approved");
                }),
            ),
            (
                "bad date month",
                exception(|map| {
                    map.insert("expires".into(), json!("2026-13-01"));
                }),
            ),
            (
                "bad date word",
                exception(|map| {
                    map.insert("expires".into(), json!("tomorrow"));
                }),
            ),
            (
                "bad date day",
                exception(|map| {
                    map.insert("expires".into(), json!("2026-02-30"));
                }),
            ),
            (
                "unknown field",
                exception(|map| {
                    map.insert("extra".into(), json!(1));
                }),
            ),
            (
                "bad selector",
                exception(|map| {
                    map.insert("rule".into(), json!("lexis"));
                }),
            ),
            ("unknown file key", json!({"exception": [], "extra": 1})),
        ],
    )?;
    assert!(mismatches.is_empty(), "{mismatches:#?}");
    Ok(())
}

#[test]
fn schema_report_accepts_a_serialised_report_and_rejects_refusals() -> Outcome {
    let all = schema();
    let report = json!({
        "run": {"tool": "0.1.0", "date": "2026-11-27T10:00:00Z", "catalogues": [], "select": ["ALL"], "ignore": [], "paths": ["src"]},
        "findings": [{
            "rule": "lexis.elif", "path": "src/a.py", "message": "m", "severity": "error", "tags": {"law": "5"},
            "span": {"start": {"line": 1, "column": 1}, "end": {"line": 1, "column": 2}, "bytes": {"start": 0, "end": 1}},
        }],
        "summary": {"total": 1, "by_severity": {"error": 1}, "by_tag": {"law": {"5": 1}}, "by_rule": {"lexis.elif": 1}},
    });
    assert!(serde_json::from_value::<Report>(report.clone()).is_ok());
    assert!(accepts(&all.report, &report)?, "{}", why(&all.report, &report)?);
    let edit = |edit: &dyn Fn(&mut Value)| {
        let mut copy = report.clone();
        edit(&mut copy);
        copy
    };
    let mismatches = agree::<Report>(
        &all.report,
        &[
            ("unknown field", edit(&|v| v["extra"] = json!(1))),
            ("bad date", edit(&|v| v["run"]["date"] = json!("yesterday"))),
            ("bad rule id", edit(&|v| v["findings"][0]["rule"] = json!("Elif"))),
            ("bad severity", edit(&|v| v["findings"][0]["severity"] = json!("fatal"))),
            (
                "bad severity key",
                edit(&|v| v["summary"]["by_severity"] = json!({"fatal": 1})),
            ),
            ("negative total", edit(&|v| v["summary"]["total"] = json!(-1))),
            (
                "missing summary",
                edit(&|v| {
                    v.as_object_mut().map(|m| m.remove("summary"));
                }),
            ),
            ("unknown finding field", edit(&|v| v["findings"][0]["extra"] = json!(1))),
        ],
    )?;
    assert!(mismatches.is_empty(), "{mismatches:#?}");
    Ok(())
}

#[test]
fn schema_catalogue_type_is_not_published_but_its_rules_are() -> Outcome {
    let catalogue: Catalogue =
        serde_json::from_value(json!({"name": "c", "version": "1", "rules": [], "profiles": []}))?;
    assert!(catalogue.rules.is_empty());
    Ok(())
}

#[test]
fn schema_selector_pattern_agrees_with_the_parser() {
    let all = schema();
    let wrapped = |text: &str| json!({"select": [text]});
    let mismatches: Vec<String> = [
        "ALL",
        "all",
        "lexis.elif",
        "lexis.*",
        "Lexis.*",
        "lexis2.x",
        "law=5",
        "law=",
        "=5",
        "a=b=c",
        "lexis",
        "Lexis",
        "lexis.",
        "lexis.[",
        "lexis.e lif",
        "ost.one-word",
        "x?.y",
        "*",
        "*.*",
        "",
    ]
    .into_iter()
    .filter_map(|text| {
        let parser = text.parse::<Selector>().is_ok();
        let schema = accepts(&all.config, &wrapped(text)).ok()?;
        (parser != schema).then(|| format!("{text:?}: parser accepts={parser}, schema accepts={schema}"))
    })
    .collect();
    assert!(mismatches.is_empty(), "{mismatches:#?}");
}

#[test]
fn schema_rule_id_pattern_agrees_with_the_parser() {
    let all = schema();
    let mismatches: Vec<String> = [
        "lexis.elif",
        "ost.one-word-module",
        "a.b",
        "Elif",
        "lexis",
        "lexis.",
        ".x",
        "A.b",
        "a.B",
        "a.b.c",
        "1a.b",
        "a.1b",
        "a.b-",
        "a.-b",
        "a-.b",
        "",
        "a.b\n",
    ]
    .into_iter()
    .filter_map(|text| {
        let parser = text.parse::<law_model::RuleId>().is_ok();
        let schema = accepts(&all.rules, &rule_with("id", json!(text)).ok()?).ok()?;
        (parser != schema).then(|| format!("{text:?}: parser accepts={parser}, schema accepts={schema}"))
    })
    .collect();
    assert!(mismatches.is_empty(), "{mismatches:#?}");
}

const FULL_PROFILE: &str = r#"[{"kind": "driver", "home": ["app/*/providers/drivers/*/*.py"], "suffix": "Driver",
    "methods": ["__aenter__", "health"], "attributes": ["spec"], "line": ["typing.Protocol"], "bases": ["typing.Protocol", "typing_extensions.Protocol"],
    "never": ["abc.ABC"], "thresholds": {"lines": 400}}]"#;

#[test]
fn schema_profiles_accept_every_field_and_reject_each_malformed_one() -> Outcome {
    let all = schema();
    let full: Value = serde_json::from_str(FULL_PROFILE)?;
    assert!(accepts(&all.profiles, &full)?, "{}", why(&all.profiles, &full)?);
    assert!(serde_json::from_value::<Vec<Profile>>(full).is_ok());
    let mismatches = agree::<Vec<Profile>>(
        &all.profiles,
        &[
            ("suffix not text", json!([{"kind": "e", "home": [], "suffix": 3}])),
            (
                "methods not a list",
                json!([{"kind": "e", "home": [], "methods": "go"}]),
            ),
            (
                "attributes not a list",
                json!([{"kind": "e", "home": [], "attributes": "spec"}]),
            ),
            (
                "line not a list",
                json!([{"kind": "e", "home": [], "line": "typing.Protocol"}]),
            ),
            (
                "bases not a list",
                json!([{"kind": "e", "home": [], "bases": "typing.Protocol"}]),
            ),
            ("bases item not text", json!([{"kind": "e", "home": [], "bases": [1]}])),
            (
                "never not a list",
                json!([{"kind": "e", "home": [], "never": "abc.ABC"}]),
            ),
            (
                "methods item not text",
                json!([{"kind": "e", "home": [], "methods": [1]}]),
            ),
            (
                "thresholds negative",
                json!([{"kind": "e", "home": [], "thresholds": {"lines": -1}}]),
            ),
            (
                "thresholds fractional",
                json!([{"kind": "e", "home": [], "thresholds": {"lines": 1.5}}]),
            ),
            (
                "thresholds text",
                json!([{"kind": "e", "home": [], "thresholds": {"lines": "many"}}]),
            ),
            (
                "thresholds not a map",
                json!([{"kind": "e", "home": [], "thresholds": [1]}]),
            ),
            ("unknown field", json!([{"kind": "e", "home": [], "lines": 3}])),
        ],
    )?;
    assert!(mismatches.is_empty(), "{mismatches:#?}");
    Ok(())
}

#[test]
fn schema_rules_accept_a_profile_check_of_every_aspect_and_reject_each_malformed_one() -> Outcome {
    let all = schema();
    for aspect in ["present", "surname", "methods", "attributes", "line", "bases", "never"] {
        let rule = rule_with(
            "check",
            json!({"engine": "profile", "kinds": ["driver", "adapter"], "aspect": aspect}),
        )?;
        assert!(accepts(&all.rules, &rule)?, "{aspect}: {}", why(&all.rules, &rule)?);
        assert!(serde_json::from_value::<Vec<Rule>>(rule).is_ok(), "{aspect}");
    }
    let mismatches = agree::<Vec<Rule>>(
        &all.rules,
        &[
            (
                "empty kinds",
                rule_with("check", json!({"engine": "profile", "kinds": [], "aspect": "present"}))?,
            ),
            (
                "no kinds",
                rule_with("check", json!({"engine": "profile", "aspect": "present"}))?,
            ),
            (
                "no aspect",
                rule_with("check", json!({"engine": "profile", "kinds": ["driver"]}))?,
            ),
            (
                "unknown aspect",
                rule_with(
                    "check",
                    json!({"engine": "profile", "kinds": ["driver"], "aspect": "shape"}),
                )?,
            ),
            (
                "bad kind",
                rule_with(
                    "check",
                    json!({"engine": "profile", "kinds": ["Driver"], "aspect": "present"}),
                )?,
            ),
            (
                "kinds as text",
                rule_with(
                    "check",
                    json!({"engine": "profile", "kinds": "driver", "aspect": "present"}),
                )?,
            ),
            (
                "unknown field",
                rule_with(
                    "check",
                    json!({"engine": "profile", "kinds": ["driver"], "aspect": "present", "extra": 1}),
                )?,
            ),
        ],
    )?;
    assert!(mismatches.is_empty(), "{mismatches:#?}");
    Ok(())
}

#[test]
fn schema_rules_accept_a_metric_check_of_every_measure_and_reject_each_malformed_one() -> Outcome {
    let all = schema();
    for measure in ["lines", "classes", "companions", "functions", "methods"] {
        let rule = rule_with(
            "check",
            json!({"engine": "metric", "kinds": ["driver", "adapter"], "measure": measure, "over": "lines"}),
        )?;
        assert!(accepts(&all.rules, &rule)?, "{measure}: {}", why(&all.rules, &rule)?);
        assert!(serde_json::from_value::<Vec<Rule>>(rule).is_ok(), "{measure}");
    }
    let ok = json!({"engine": "metric", "kinds": ["driver"], "measure": "lines", "over": "lines"});
    let with = |key: &str, value: Value| {
        let mut check = ok.clone();
        check[key] = value;
        rule_with("check", check)
    };
    let mismatches = agree::<Vec<Rule>>(
        &all.rules,
        &[
            ("empty kinds", with("kinds", json!([]))?),
            ("kinds as text", with("kinds", json!("driver"))?),
            ("bad kind", with("kinds", json!(["Driver"]))?),
            ("unknown measure", with("measure", json!("depth"))?),
            ("measure cased", with("measure", json!("Lines"))?),
            ("empty over", with("over", json!(""))?),
            ("over not text", with("over", json!(3))?),
            ("unknown field", with("extra", json!(1))?),
            (
                "no over",
                rule_with(
                    "check",
                    json!({"engine": "metric", "kinds": ["driver"], "measure": "lines"}),
                )?,
            ),
            (
                "no measure",
                rule_with(
                    "check",
                    json!({"engine": "metric", "kinds": ["driver"], "over": "lines"}),
                )?,
            ),
            (
                "no kinds",
                rule_with(
                    "check",
                    json!({"engine": "metric", "measure": "lines", "over": "lines"}),
                )?,
            ),
        ],
    )?;
    assert!(mismatches.is_empty(), "{mismatches:#?}");
    Ok(())
}

#[test]
fn schema_rules_accept_ruff_settings_of_any_toml_value_and_reject_what_toml_cannot_hold() -> Outcome {
    let all = schema();
    let ruff = |settings: Value| {
        rule_with(
            "check",
            json!({"engine": "ruff", "code": "TID251", "settings": settings}),
        )
    };
    for settings in [
        json!({}),
        json!({"line-length": 100}),
        json!({"lint.flake8-tidy-imports.banned-api": {"os.environ": {"msg": "m"}}}),
        json!({"lint.select": ["E", "F"], "lint.preview": true, "lint.x": 1.5}),
    ] {
        let rule = ruff(settings.clone())?;
        assert!(accepts(&all.rules, &rule)?, "{settings}: {}", why(&all.rules, &rule)?);
        assert!(serde_json::from_value::<Vec<Rule>>(rule).is_ok(), "{settings}");
    }
    let mismatches = agree::<Vec<Rule>>(
        &all.rules,
        &[
            ("null value", ruff(json!({"lint.x": null}))?),
            ("null nested in a table", ruff(json!({"lint.x": {"a": null}}))?),
            ("null in a list", ruff(json!({"lint.x": [1, null]}))?),
            ("a list, not a map", ruff(json!([1]))?),
            ("text, not a map", ruff(json!("x"))?),
        ],
    )?;
    assert!(mismatches.is_empty(), "{mismatches:#?}");
    Ok(())
}
