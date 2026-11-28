mod common;

use common::{Outcome, ids, ruff, rule, tach, ty};
use law_external::{RuffRoute, TachRoute, TyRoute};
use serde_json::json;

#[test]
fn route_ruff_code_goes_to_the_rule_naming_it() -> Outcome {
    let rules = [ruff("a.print", "T201", None)?, ruff("a.exit", "PLW1514", None)?];
    let route = RuffRoute::new(&rules, |_| true);
    assert_eq!(ids(route.route("T201", "`print` found")), ["a.print"]);
    assert!(route.route("E501", "x").is_empty());
    Ok(())
}

#[test]
fn route_ruff_message_splits_one_code_between_rules() -> Outcome {
    let rules = [
        ruff("a.environ", "TID251", Some("os.environ"))?,
        ruff("a.skip", "TID251", Some("pytest.skip"))?,
        ruff("a.rest", "TID251", None)?,
    ];
    let route = RuffRoute::new(&rules, |_| true);
    assert_eq!(ids(route.route("TID251", "`os.environ` is banned")), ["a.environ"]);
    assert_eq!(ids(route.route("TID251", "`pytest.skip` is banned")), ["a.skip"]);
    assert_eq!(
        ids(route.route("TID251", "`os.getenv` is banned")),
        ["a.rest"],
        "no sibling's message found: the rule without one takes it"
    );
    Ok(())
}

#[test]
fn route_ruff_message_is_found_not_fully_matched() -> Outcome {
    let rules = [ruff("a.skip", "TID251", Some("skip"))?];
    let route = RuffRoute::new(&rules, |_| true);
    assert_eq!(ids(route.route("TID251", "`pytest.skip` is banned")), ["a.skip"]);
    Ok(())
}

#[test]
fn route_ruff_message_rule_alone_leaves_other_messages_unrouted() -> Outcome {
    let rules = [ruff("a.skip", "TID251", Some("pytest.skip"))?];
    let route = RuffRoute::new(&rules, |_| true);
    assert!(route.route("TID251", "`os.environ` is banned").is_empty());
    Ok(())
}

#[test]
fn route_ruff_two_matching_messages_go_to_both() -> Outcome {
    let rules = [
        ruff("a.one", "TID251", Some("banned"))?,
        ruff("a.two", "TID251", Some("os"))?,
    ];
    let route = RuffRoute::new(&rules, |_| true);
    assert_eq!(ids(route.route("TID251", "`os.environ` is banned")), ["a.one", "a.two"]);
    Ok(())
}

#[test]
fn route_ruff_unselected_rules_take_nothing_and_name_no_code() -> Outcome {
    let rules = [ruff("a.print", "T201", None)?, ruff("a.bare", "E722", None)?];
    let route = RuffRoute::new(&rules, |rule| rule.id.as_str() == "a.print");
    assert!(route.route("E722", "x").is_empty());
    assert_eq!(ids(route.rules()), ["a.print"]);
    assert_eq!(route.codes().iter().map(|c| c.as_str()).collect::<Vec<_>>(), ["T201"]);
    Ok(())
}

#[test]
fn route_ruff_codes_are_once_each_and_ordered() -> Outcome {
    let rules = [
        ruff("a.z", "T201", None)?,
        ruff("a.skip", "TID251", Some("skip"))?,
        ruff("a.environ", "TID251", Some("environ"))?,
        ruff("a.b", "B904", None)?,
    ];
    let route = RuffRoute::new(&rules, |_| true);
    assert_eq!(
        route.codes().iter().map(|c| c.as_str()).collect::<Vec<_>>(),
        ["B904", "T201", "TID251"]
    );
    Ok(())
}

#[test]
fn route_ruff_ignores_rules_of_other_engines_and_finds_a_rules_code() -> Outcome {
    let pattern = rule(
        "a.elif",
        &json!({"engine": "pattern", "rule": {"kind": "elif_clause"}}),
        &json!({"safety": "none", "reason": "x"}),
    )?;
    let print = ruff("a.print", "T201", None)?;
    let route = RuffRoute::new([&pattern, &print], |_| true);
    assert_eq!(ids(route.rules()), ["a.print"]);
    assert_eq!(route.code(&print.id).map(law_model::Code::as_str), Some("T201"));
    assert!(route.code(&pattern.id).is_none());
    Ok(())
}

#[test]
fn route_ty_named_rule_takes_its_diagnostics_else_the_unnamed() -> Outcome {
    let rules = [ty("a.ret", Some("invalid-explicit-return"))?, ty("a.any", None)?];
    let route = TyRoute::new(&rules, |_| true);
    assert_eq!(ids(route.route("invalid-explicit-return")), ["a.ret"]);
    assert_eq!(ids(route.route("unresolved-reference")), ["a.any"]);
    Ok(())
}

#[test]
fn route_ty_unnamed_alone_takes_everything_and_named_alone_takes_only_its_own() -> Outcome {
    let all = [ty("a.any", None)?];
    assert_eq!(ids(TyRoute::new(&all, |_| true).route("whatever")), ["a.any"]);
    let named = [ty("a.ret", Some("invalid-explicit-return"))?];
    assert!(TyRoute::new(&named, |_| true).route("whatever").is_empty());
    Ok(())
}

#[test]
fn route_ty_unselected_rule_takes_nothing() -> Outcome {
    let rules = [ty("a.any", None)?];
    let route = TyRoute::new(&rules, |_| false);
    assert!(route.route("x").is_empty() && route.rules().is_empty());
    Ok(())
}

#[test]
fn route_tach_named_rule_takes_its_kind_else_the_unnamed() -> Outcome {
    let rules = [tach("a.layer", Some("LayerViolation"))?, tach("a.any", None)?];
    let route = TachRoute::new(&rules, |_| true);
    assert_eq!(ids(route.route("LayerViolation")), ["a.layer"]);
    assert_eq!(ids(route.route("UndeclaredDependency")), ["a.any"]);
    Ok(())
}

#[test]
fn route_tach_unnamed_alone_takes_everything_and_named_alone_takes_only_its_own() -> Outcome {
    let all = [tach("a.any", None)?];
    assert_eq!(ids(TachRoute::new(&all, |_| true).route("Whatever")), ["a.any"]);
    let named = [tach("a.layer", Some("LayerViolation"))?];
    assert!(TachRoute::new(&named, |_| true).route("Whatever").is_empty());
    Ok(())
}

#[test]
fn route_tach_two_rules_of_one_kind_both_take_it() -> Outcome {
    let rules = [tach("a.one", Some("Visibility"))?, tach("a.two", Some("Visibility"))?];
    assert_eq!(
        ids(TachRoute::new(&rules, |_| true).route("Visibility")),
        ["a.one", "a.two"]
    );
    Ok(())
}

#[test]
fn route_tach_unselected_rules_take_nothing_and_name_no_kind() -> Outcome {
    let rules = [
        tach("a.layer", Some("LayerViolation"))?,
        tach("a.cycle", Some("CircularDependency"))?,
        tach("a.any", None)?,
    ];
    let route = TachRoute::new(&rules, |rule| rule.id.as_str() == "a.layer");
    assert!(route.route("CircularDependency").is_empty());
    assert!(route.route("Whatever").is_empty());
    assert_eq!(ids(route.rules()), ["a.layer"]);
    assert_eq!(route.codes(), [Some("LayerViolation")]);
    Ok(())
}

#[test]
fn route_tach_codes_keep_the_unnamed_as_none_and_ignore_other_engines() -> Outcome {
    let pattern = rule(
        "a.elif",
        &json!({"engine": "pattern", "rule": {"kind": "elif_clause"}}),
        &json!({"safety": "none", "reason": "x"}),
    )?;
    let rules = [pattern, tach("a.layer", Some("LayerViolation"))?, tach("a.any", None)?];
    let route = TachRoute::new(&rules, |_| true);
    assert_eq!(ids(route.rules()), ["a.layer", "a.any"]);
    assert_eq!(route.codes(), [Some("LayerViolation"), None]);
    Ok(())
}
