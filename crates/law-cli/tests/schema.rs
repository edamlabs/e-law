mod common;

use common::{Outcome, Project};
use law_config::schema;
use serde_json::{Value, json};

fn printed(project: &Project, args: &[&str]) -> Result<(Value, common::Out), Box<dyn std::error::Error>> {
    let mut all = vec!["schema"];
    all.extend_from_slice(args);
    let out = project.run(&all)?;
    Ok((serde_json::from_str(&out.stdout)?, out))
}

#[test]
fn schema_each_name_prints_its_own_schema() -> Outcome {
    let schemas = schema();
    let project = Project::new()?;
    let expected = [
        ("config", schemas.config.as_value().clone()),
        ("rules", schemas.rules.as_value().clone()),
        ("profiles", schemas.profiles.as_value().clone()),
        ("exceptions", schemas.exceptions.as_value().clone()),
        ("report", schemas.report.as_value().clone()),
    ];
    for (name, value) in expected {
        let (printed, out) = printed(&project, &[name])?;
        assert_eq!(printed, value, "{name}");
        assert_eq!((out.stderr.as_str(), out.code), ("", 0), "{name}");
    }
    Ok(())
}

#[test]
fn schema_no_argument_prints_all_five_keyed_by_name() -> Outcome {
    let schemas = schema();
    let (all, out) = printed(&Project::new()?, &[])?;
    assert_eq!(
        all,
        json!({
            "config": schemas.config.as_value(),
            "rules": schemas.rules.as_value(),
            "profiles": schemas.profiles.as_value(),
            "exceptions": schemas.exceptions.as_value(),
            "report": schemas.report.as_value(),
        })
    );
    assert_eq!(out.code, 0);
    Ok(())
}

#[test]
fn schema_all_holds_exactly_the_five_names() -> Outcome {
    let (all, _) = printed(&Project::new()?, &[])?;
    let keys: Vec<&String> = all.as_object().ok_or("not an object")?.keys().collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(sorted, ["config", "exceptions", "profiles", "report", "rules"]);
    Ok(())
}

#[test]
fn schema_is_json_schema_and_pretty_printed() -> Outcome {
    let out = Project::new()?.run(&["schema", "config"])?;
    assert!(out.stdout.starts_with("{\n  \""), "{}", out.stdout);
    assert!(out.stdout.ends_with("}\n"));
    let value: Value = serde_json::from_str(&out.stdout)?;
    assert!(value["$schema"].is_string(), "{value}");
    Ok(())
}

#[test]
fn schema_needs_no_project_and_is_deterministic() -> Outcome {
    let project = Project::new()?;
    let first = project.run(&["schema"])?;
    let second = project.run(&["schema"])?;
    assert_eq!(first.code, 0);
    assert_eq!(first.stdout, second.stdout);
    Ok(())
}

#[test]
fn schema_unknown_name_is_a_usage_error() -> Outcome {
    let out = Project::new()?.run(&["schema", "nope"])?;
    assert_eq!(out.code, 2);
    assert_eq!(out.stdout, "");
    assert!(out.stderr.contains("nope"), "{}", out.stderr);
    Ok(())
}
