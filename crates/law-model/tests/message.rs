use law_model::{Message, Refusal};

type Outcome = Result<(), Box<dyn std::error::Error>>;

#[test]
fn message_accepts_one_line() -> Outcome {
    let message: Message = "elif is a decision table nobody wrote down".parse()?;
    assert_eq!(message.as_str(), "elif is a decision table nobody wrote down");
    Ok(())
}

#[test]
fn message_refuses_newline_as_multiline() {
    for text in ["a\nb", "a\r\nb", "a\rb", "a\n"] {
        assert_eq!(
            text.parse::<Message>().err().map(|invalid| invalid.refusal),
            Some(Refusal::MultilineMessage),
            "{text:?}"
        );
    }
}

#[test]
fn message_refuses_empty() {
    assert_eq!(
        "".parse::<Message>().err().map(|invalid| invalid.refusal),
        Some(Refusal::EmptyText)
    );
}

#[test]
fn message_deserialisation_refuses_multiline() {
    assert!(serde_json::from_str::<Message>("\"a\\nb\"").is_err());
}

#[test]
fn message_round_trips() -> Outcome {
    let message: Message = "one line".parse()?;
    let back: Message = serde_json::from_str(&serde_json::to_string(&message)?)?;
    assert_eq!(message, back);
    Ok(())
}
