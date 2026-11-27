use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

use crate::invalid::{Invalid, Refusal};

/// What a rule says when it fires: one non-empty line.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Message(String);

impl Message {
    /// The message as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for Message {
    type Err = Invalid;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match (text.is_empty(), text.contains(['\n', '\r'])) {
            (true, _) => Err(Invalid::new(Refusal::EmptyText, "a message must say something")),
            (_, true) => Err(Invalid::new(
                Refusal::MultilineMessage,
                format!("{text:?} holds a newline"),
            )),
            _ => Ok(Self(text.to_owned())),
        }
    }
}

impl TryFrom<String> for Message {
    type Error = Invalid;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<Message> for String {
    fn from(message: Message) -> Self {
        message.0
    }
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl JsonSchema for Message {
    fn schema_name() -> Cow<'static, str> {
        "Message".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": "^[^\\n\\r]+$",
            "description": "One non-empty line.",
        })
    }
}
