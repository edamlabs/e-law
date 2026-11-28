use std::borrow::Cow;
use std::hash::{Hash, Hasher};

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::invalid::{Invalid, Refusal};

const NULL: &str = "null";
const EQUALS: &str = " = ";
const JOIN: &str = ", ";

/// A value a tool's setting takes: any TOML value, so no null at any depth.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Value", into = "Value")]
pub struct Setting(Value);

impl Setting {
    fn nulled(value: &Value) -> bool {
        match value {
            Value::Null => true,
            Value::Array(items) => items.iter().any(Self::nulled),
            Value::Object(entries) => entries.values().any(Self::nulled),
            _ => false,
        }
    }

    fn render(value: &Value) -> String {
        match value {
            Value::Null => NULL.to_owned(),
            Value::Array(items) => format!("[{}]", items.iter().map(Self::render).collect::<Vec<_>>().join(JOIN)),
            Value::Object(entries) => format!(
                "{{{}}}",
                entries
                    .iter()
                    .map(|(key, value)| format!("{}{EQUALS}{}", Value::String(key.clone()), Self::render(value)))
                    .collect::<Vec<_>>()
                    .join(JOIN)
            ),
            scalar => scalar.to_string(),
        }
    }

    /// The value as TOML writes it inline: a table `{"key" = value}`, a list `[a, b]`, a quoted string.
    #[must_use]
    pub fn toml(&self) -> String {
        Self::render(&self.0)
    }
}

impl Hash for Setting {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_string().hash(state);
    }
}

impl TryFrom<Value> for Setting {
    type Error = Invalid;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        if Self::nulled(&value) {
            Err(Invalid::new(Refusal::BadSetting, "TOML has no null"))
        } else {
            Ok(Self(value))
        }
    }
}

impl From<Setting> for Value {
    fn from(setting: Setting) -> Self {
        setting.0
    }
}

impl JsonSchema for Setting {
    fn schema_name() -> Cow<'static, str> {
        "Setting".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let this = generator.subschema_for::<Self>();
        json_schema!({
            "type": ["string", "number", "boolean", "array", "object"],
            "items": this,
            "additionalProperties": this,
            "description": "A value a tool's setting takes: any TOML value, so no null at any depth.",
        })
    }
}
