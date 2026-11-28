use law_model::{Exception, Refusal};
use serde_json::json;

type Outcome = Result<(), Box<dyn std::error::Error>>;

fn base() -> serde_json::Value {
    json!({"rule": "lexis.elif", "path": "src/legacy/**", "reason": "migrating", "approved": "damien"})
}

fn failure(value: &serde_json::Value) -> Option<String> {
    serde_json::from_value::<Exception>(value.clone())
        .err()
        .map(|error| error.to_string())
}

fn mutate(edit: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>)) -> serde_json::Value {
    let mut value = base();
    if let Some(map) = value.as_object_mut() {
        edit(map);
    }
    value
}

#[test]
fn exception_with_path_loads() -> Outcome {
    let exception: Exception = serde_json::from_value(base())?;
    assert_eq!(exception.rule().to_string(), "lexis.elif");
    assert_eq!(exception.path().map(law_model::Glob::as_str), Some("src/legacy/**"));
    assert!(exception.symbol().is_none() && exception.expires().is_none());
    assert_eq!((exception.reason(), exception.approved()), ("migrating", "damien"));
    Ok(())
}

#[test]
fn exception_with_symbol_only_loads() -> Outcome {
    let value = mutate(|map| {
        map.remove("path");
        map.insert("symbol".into(), json!("Legacy.run"));
    });
    let exception: Exception = serde_json::from_value(value)?;
    assert_eq!(exception.symbol(), Some("Legacy.run"));
    assert!(exception.path().is_none());
    Ok(())
}

#[test]
fn exception_without_path_or_symbol_is_refused_by_name() {
    let value = mutate(|map| {
        map.remove("path");
    });
    let message = failure(&value).unwrap_or_default();
    assert!(
        message.contains(Refusal::ExceptionWithoutTarget.code()),
        "message: {message:?}"
    );
}

#[test]
fn exception_empty_reason_is_refused() {
    let message = failure(&mutate(|map| {
        map.insert("reason".into(), json!(""));
    }))
    .unwrap_or_default();
    assert!(
        message.contains(Refusal::EmptyText.code()) && message.contains("reason"),
        "{message:?}"
    );
}

#[test]
fn exception_empty_approver_is_refused() {
    let message = failure(&mutate(|map| {
        map.insert("approved".into(), json!(""));
    }))
    .unwrap_or_default();
    assert!(
        message.contains(Refusal::EmptyText.code()) && message.contains("approver"),
        "{message:?}"
    );
}

#[test]
fn exception_empty_symbol_is_refused() {
    assert!(
        failure(&mutate(|map| {
            map.insert("symbol".into(), json!(""));
        }))
        .is_some()
    );
}

#[test]
fn exception_missing_reason_or_approver_is_refused() {
    assert!(
        failure(&mutate(|map| {
            map.remove("reason");
        }))
        .is_some()
    );
    assert!(
        failure(&mutate(|map| {
            map.remove("approved");
        }))
        .is_some()
    );
}

#[test]
fn exception_bad_date_is_refused() {
    for date in ["2026-13-01", "tomorrow", "2026-02-30", "26-01-01", ""] {
        assert!(
            failure(&mutate(|map| {
                map.insert("expires".into(), json!(date));
            }))
            .is_some(),
            "{date:?}"
        );
    }
}

#[test]
fn exception_good_date_loads() -> Outcome {
    let exception: Exception = serde_json::from_value(mutate(|map| {
        map.insert("expires".into(), json!("2027-03-01"));
    }))?;
    assert_eq!(
        exception.expires().map(|date| date.to_string()),
        Some("2027-03-01".to_owned())
    );
    Ok(())
}

#[test]
fn exception_unknown_field_is_refused() {
    assert!(
        failure(&mutate(|map| {
            map.insert("extra".into(), json!(1));
        }))
        .is_some()
    );
}

#[test]
fn exception_bad_selector_or_glob_is_refused() {
    assert!(
        failure(&mutate(|map| {
            map.insert("rule".into(), json!("lexis"));
        }))
        .is_some()
    );
    assert!(
        failure(&mutate(|map| {
            map.insert("path".into(), json!("a/["));
        }))
        .is_some()
    );
}

#[test]
fn exception_round_trips() -> Outcome {
    for value in [
        base(),
        mutate(|map| {
            map.insert("expires".into(), json!("2027-03-01"));
            map.insert("symbol".into(), json!("f"));
        }),
    ] {
        let exception: Exception = serde_json::from_value(value)?;
        let back: Exception = serde_json::from_str(&serde_json::to_string(&exception)?)?;
        assert_eq!(back, exception);
    }
    Ok(())
}

#[derive(serde::Deserialize)]
struct File {
    exception: Vec<Exception>,
}

fn toml_with(expires: &str) -> Result<File, toml::de::Error> {
    toml::from_str(&format!(
        "[[exception]]\nrule = \"law=prompts\"\npath = \"app/p/**\"\nreason = \"r\"\napproved = \"a\"\nexpires = {expires}\n"
    ))
}

fn first_expiry(file: &File) -> Option<String> {
    file.exception
        .first()
        .and_then(Exception::expires)
        .map(|date| date.to_string())
}

#[test]
fn exception_toml_quoted_date_loads() -> Outcome {
    let file = toml_with("\"2027-03-01\"")?;
    assert_eq!(file.exception.len(), 1);
    assert_eq!(first_expiry(&file), Some("2027-03-01".to_owned()));
    Ok(())
}

#[test]
fn exception_toml_native_date_loads() -> Outcome {
    let file = toml_with("2027-03-01")?;
    assert_eq!(first_expiry(&file), Some("2027-03-01".to_owned()));
    Ok(())
}
