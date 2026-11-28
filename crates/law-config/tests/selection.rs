use law_config::{Config, Selection};
use law_model::{Rule, Selector};
use serde_json::json;

type Outcome = Result<(), Box<dyn std::error::Error>>;

fn rule(id: &str, law: &str) -> Result<Rule, Box<dyn std::error::Error>> {
    Ok(serde_json::from_value(json!({
        "id": id,
        "message": "m",
        "tags": {"law": law},
        "check": {"engine": "pattern", "rule": {"kind": "x"}},
        "fix": {"safety": "safe"},
    }))?)
}

fn selectors(texts: &[&str]) -> Result<Vec<Selector>, Box<dyn std::error::Error>> {
    Ok(texts.iter().map(|text| text.parse()).collect::<Result<_, _>>()?)
}

fn config(select: &[&str], ignore: &[&str]) -> Result<Config, Box<dyn std::error::Error>> {
    Ok(Config {
        select: selectors(select)?,
        ignore: selectors(ignore)?,
        ..Config::default()
    })
}

#[test]
fn selection_default_admits_everything() -> Outcome {
    let selection = Selection::new(&Config::default(), None, None);
    assert!(selection.admits(&rule("lexis.elif", "5")?));
    assert!(selection.admits(&rule("ost.x", "1")?));
    Ok(())
}

#[test]
fn selection_ignore_beats_select() -> Outcome {
    let selection = Selection::new(&config(&["ALL"], &["lexis.elif"])?, None, None);
    assert!(!selection.admits(&rule("lexis.elif", "5")?));
    assert!(selection.admits(&rule("lexis.loop", "5")?));
    let selection = Selection::new(&config(&["lexis.elif"], &["lexis.elif"])?, None, None);
    assert!(!selection.admits(&rule("lexis.elif", "5")?));
    let selection = Selection::new(&config(&["law=5"], &["lexis.*"])?, None, None);
    assert!(!selection.admits(&rule("lexis.elif", "5")?));
    assert!(selection.admits(&rule("ost.x", "5")?));
    Ok(())
}

#[test]
fn selection_select_chooses_only_what_it_names() -> Outcome {
    let selection = Selection::new(&config(&["lexis.*"], &[])?, None, None);
    assert!(selection.admits(&rule("lexis.elif", "5")?));
    assert!(!selection.admits(&rule("ost.x", "5")?));
    assert!(!selection.admits(&rule("lexis2.x", "5")?));
    Ok(())
}

#[test]
fn selection_empty_select_admits_nothing() -> Outcome {
    let selection = Selection::new(&config(&[], &[])?, None, None);
    assert!(!selection.admits(&rule("lexis.elif", "5")?));
    Ok(())
}

#[test]
fn selection_command_line_select_replaces_config_select() -> Outcome {
    let selection = Selection::new(&config(&["lexis.*"], &[])?, Some(selectors(&["ost.*"])?), None);
    assert!(selection.admits(&rule("ost.x", "5")?));
    assert!(!selection.admits(&rule("lexis.elif", "5")?));
    assert_eq!(selection.select, selectors(&["ost.*"])?);
    Ok(())
}

#[test]
fn selection_command_line_ignore_replaces_config_ignore() -> Outcome {
    let selection = Selection::new(&config(&["ALL"], &["lexis.*"])?, None, Some(selectors(&["ost.*"])?));
    assert!(
        selection.admits(&rule("lexis.elif", "5")?),
        "config ignore must be replaced, not merged"
    );
    assert!(!selection.admits(&rule("ost.x", "5")?));
    Ok(())
}

#[test]
fn selection_command_line_empty_list_still_replaces() -> Outcome {
    let selection = Selection::new(&config(&["ALL"], &["lexis.*"])?, None, Some(Vec::new()));
    assert!(selection.admits(&rule("lexis.elif", "5")?));
    Ok(())
}

#[test]
fn selection_command_line_ignore_still_beats_command_line_select() -> Outcome {
    let selection = Selection::new(
        &Config::default(),
        Some(selectors(&["ALL"])?),
        Some(selectors(&["law=5"])?),
    );
    assert!(!selection.admits(&rule("lexis.elif", "5")?));
    assert!(selection.admits(&rule("lexis.elif", "6")?));
    Ok(())
}
