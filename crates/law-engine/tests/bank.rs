use law_engine::{Bank, Fault};

const MINIMAL: &str =
    r#"{"aliases": {}, "factories": {}, "wrappers": [], "classes": {"builtins.object": {"bases": [], "methods": []}}}"#;
const CHILD: &str = r#"{"aliases": {}, "factories": {}, "wrappers": [], "classes": {"builtins.object": {"bases": [], "methods": []}, "builtins.int": {"bases": ["builtins.object"], "methods": ["bit_length"]}}}"#;
const DANGLING: &str = r#"{"aliases": {}, "factories": {}, "wrappers": [], "classes": {"builtins.int": {"bases": ["builtins.number"], "methods": []}}}"#;

fn message(text: &str) -> Option<String> {
    match Bank::decode(text) {
        Err(Fault::Bank { message }) => Some(message),
        _ => None,
    }
}

#[test]
fn bank_decode_accepts_a_minimal_valid_bank() {
    assert!(Bank::decode(MINIMAL).is_ok());
}

#[test]
fn bank_decode_accepts_a_class_whose_base_the_bank_holds() {
    assert!(Bank::decode(CHILD).is_ok());
}

#[test]
fn bank_decode_refuses_a_class_naming_a_base_the_bank_lacks() {
    assert_eq!(
        message(DANGLING).as_deref(),
        Some("`builtins.int` names the base `builtins.number`, which it does not hold")
    );
}

#[test]
fn bank_decode_refuses_malformed_data() {
    for text in [
        "",
        "{",
        "[]",
        "{\"aliases\": {}}",
        r#"{"aliases": {}, "factories": {}, "wrappers": [], "classes": {"a": {"bases": [], "methods": [], "extra": 1}}}"#,
    ] {
        assert!(message(text).is_some(), "accepted: {text:?}");
    }
}
