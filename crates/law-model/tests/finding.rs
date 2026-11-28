use law_model::{Edit, Finding, Fix, Safety};
use serde_json::{Value, json};

type Outcome = Result<(), Box<dyn std::error::Error>>;

fn finding(rule: &str, path: &str, start: usize, message: &str) -> Value {
    json!({
        "rule": rule,
        "path": path,
        "span": {
            "start": {"line": 1, "column": 1},
            "end": {"line": 1, "column": 2},
            "bytes": {"start": start, "end": start + 1},
        },
        "message": message,
        "severity": "error",
        "tags": {"law": "5"},
    })
}

fn parsed(value: Value) -> Result<Finding, Box<dyn std::error::Error>> {
    Ok(serde_json::from_value(value)?)
}

fn pool() -> Result<Vec<Finding>, Box<dyn std::error::Error>> {
    [
        ("lexis.b", "src/b.py", 5, "m"),
        ("lexis.a", "src/b.py", 5, "m"),
        ("lexis.a", "src/b.py", 2, "m"),
        ("lexis.a", "src/a.py", 9, "m"),
        ("lexis.a", "src/a.py", 9, "other"),
        ("lexis.a", "src/a.py", 1, "m"),
    ]
    .into_iter()
    .map(|(rule, path, start, message)| parsed(finding(rule, path, start, message)))
    .collect()
}

#[test]
fn finding_orders_by_path_then_start_then_rule() -> Outcome {
    let a_late = parsed(finding("lexis.a", "src/a.py", 9, "m"))?;
    let b_early = parsed(finding("lexis.a", "src/b.py", 1, "m"))?;
    assert!(a_late < b_early, "path first");
    let early = parsed(finding("lexis.z", "src/a.py", 1, "m"))?;
    let late = parsed(finding("lexis.a", "src/a.py", 2, "m"))?;
    assert!(early < late, "start byte before rule");
    let first = parsed(finding("lexis.a", "src/a.py", 1, "m"))?;
    let second = parsed(finding("lexis.b", "src/a.py", 1, "m"))?;
    assert!(first < second, "rule last");
    Ok(())
}

#[test]
fn finding_order_is_independent_of_input_order() -> Outcome {
    let pool = pool()?;
    let mut forward = pool.clone();
    forward.sort();
    let mut backward: Vec<Finding> = pool.iter().rev().cloned().collect();
    backward.sort();
    let mut rotated = pool.clone();
    rotated.rotate_left(3);
    rotated.sort();
    assert_eq!(forward, backward);
    assert_eq!(forward, rotated);
    Ok(())
}

#[test]
fn finding_order_is_total_and_consistent_with_eq() -> Outcome {
    let pool = pool()?;
    for left in &pool {
        for right in &pool {
            assert_eq!(left.cmp(right), right.cmp(left).reverse());
            assert_eq!(left.cmp(right) == std::cmp::Ordering::Equal, left == right);
            for third in &pool {
                if left <= right && right <= third {
                    assert!(left <= third);
                }
            }
        }
    }
    Ok(())
}

#[test]
fn finding_ties_on_key_break_by_content() -> Outcome {
    let one = parsed(finding("lexis.a", "src/a.py", 9, "m"))?;
    let other = parsed(finding("lexis.a", "src/a.py", 9, "other"))?;
    assert_ne!(one.cmp(&other), std::cmp::Ordering::Equal);
    Ok(())
}

#[test]
fn finding_round_trips_with_and_without_fix() -> Outcome {
    let plain = parsed(finding("lexis.a", "src/a.py", 1, "m"))?;
    let mut with_fix = finding("lexis.a", "src/a.py", 1, "m");
    with_fix["fix"] = json!({"safety": "unsafe", "edits": [
        {"edit": "replace", "path": "src/a.py", "bytes": {"start": 0, "end": 1}, "text": "x"},
        {"edit": "create", "path": "src/new.py", "text": ""},
        {"edit": "move", "from": "src/a.py", "to": "src/b/a.py"},
    ]});
    let fixed = parsed(with_fix)?;
    for item in [plain, fixed] {
        let back: Finding = serde_json::from_str(&serde_json::to_string(&item)?)?;
        assert_eq!(back, item);
    }
    Ok(())
}

#[test]
fn finding_omits_absent_fix_when_serialised() -> Outcome {
    let json = serde_json::to_value(parsed(finding("lexis.a", "src/a.py", 1, "m"))?)?;
    assert!(json.get("fix").is_none());
    Ok(())
}

#[test]
fn finding_refuses_unknown_field_and_bad_id() {
    let mut extra = finding("lexis.a", "src/a.py", 1, "m");
    extra["extra"] = json!(1);
    assert!(parsed(extra).is_err());
    assert!(parsed(finding("Lexis", "src/a.py", 1, "m")).is_err());
}

#[test]
fn edit_variants_round_trip_and_order_deterministically() -> Outcome {
    let edits: Vec<Edit> = serde_json::from_value(json!([
        {"edit": "move", "from": "a", "to": "b"},
        {"edit": "create", "path": "n", "text": "t"},
        {"edit": "replace", "path": "a", "bytes": {"start": 3, "end": 4}, "text": "y"},
        {"edit": "replace", "path": "a", "bytes": {"start": 1, "end": 2}, "text": "x"},
    ]))?;
    let back: Vec<Edit> = serde_json::from_str(&serde_json::to_string(&edits)?)?;
    assert_eq!(back, edits);
    let mut sorted = edits.clone();
    sorted.sort();
    let mut reversed: Vec<Edit> = edits.iter().rev().cloned().collect();
    reversed.sort();
    assert_eq!(sorted, reversed);
    assert!(matches!(sorted.first(), Some(Edit::Replace { bytes, .. }) if bytes.start == 1));
    Ok(())
}

#[test]
fn edit_refuses_unknown_kind_and_unknown_field() {
    assert!(serde_json::from_value::<Edit>(json!({"edit": "delete", "path": "a"})).is_err());
    assert!(serde_json::from_value::<Edit>(json!({"edit": "create", "path": "a", "text": "", "extra": 1})).is_err());
}

#[test]
fn fix_round_trips_and_refuses_unknown_safety() -> Outcome {
    let fix: Fix = serde_json::from_value(json!({"safety": "safe", "edits": []}))?;
    assert_eq!(fix.safety, Safety::Safe);
    let back: Fix = serde_json::from_str(&serde_json::to_string(&fix)?)?;
    assert_eq!(back, fix);
    assert!(serde_json::from_value::<Fix>(json!({"safety": "none", "edits": []})).is_err());
    assert!(serde_json::from_value::<Fix>(json!({"safety": "safe", "edits": [], "extra": 1})).is_err());
    Ok(())
}
