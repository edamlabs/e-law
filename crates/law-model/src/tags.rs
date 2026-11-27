use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::{self, SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Free labels on a rule: each key holds a set of strings the engine filters on and never interprets.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Tags(BTreeMap<String, Values>);

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Values(BTreeSet<String>);

struct Word(String);

struct WordVisitor;

struct ValuesVisitor;

impl Tags {
    /// Whether `key` holds `value`.
    #[must_use]
    pub fn contains(&self, key: &str, value: &str) -> bool {
        self.0.get(key).is_some_and(|values| values.0.contains(value))
    }

    /// Every key with its values, in order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, impl Iterator<Item = &str>)> {
        self.0
            .iter()
            .map(|(key, values)| (key.as_str(), values.0.iter().map(String::as_str)))
    }
}

impl Visitor<'_> for WordVisitor {
    type Value = Word;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a string or an integer")
    }

    fn visit_str<E: de::Error>(self, text: &str) -> Result<Word, E> {
        Ok(Word(text.to_owned()))
    }

    fn visit_string<E: de::Error>(self, text: String) -> Result<Word, E> {
        Ok(Word(text))
    }

    fn visit_i64<E: de::Error>(self, number: i64) -> Result<Word, E> {
        Ok(Word(number.to_string()))
    }

    fn visit_u64<E: de::Error>(self, number: u64) -> Result<Word, E> {
        Ok(Word(number.to_string()))
    }
}

impl<'de> Deserialize<'de> for Word {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(WordVisitor)
    }
}

impl<'de> Visitor<'de> for ValuesVisitor {
    type Value = Values;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a string, an integer, or a list of them")
    }

    fn visit_str<E: de::Error>(self, text: &str) -> Result<Values, E> {
        Ok(Values(BTreeSet::from([text.to_owned()])))
    }

    fn visit_string<E: de::Error>(self, text: String) -> Result<Values, E> {
        Ok(Values(BTreeSet::from([text])))
    }

    fn visit_i64<E: de::Error>(self, number: i64) -> Result<Values, E> {
        Ok(Values(BTreeSet::from([number.to_string()])))
    }

    fn visit_u64<E: de::Error>(self, number: u64) -> Result<Values, E> {
        Ok(Values(BTreeSet::from([number.to_string()])))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Values, A::Error> {
        let mut values = BTreeSet::new();
        while let Some(Word(word)) = seq.next_element()? {
            values.insert(word);
        }
        Ok(Values(values))
    }
}

impl<'de> Deserialize<'de> for Values {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(ValuesVisitor)
    }
}

impl Serialize for Values {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if let Some(only) = self.0.first().filter(|_| self.0.len() == 1) {
            serializer.serialize_str(only)
        } else {
            let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
            self.0.iter().try_for_each(|value| seq.serialize_element(value))?;
            seq.end()
        }
    }
}

impl JsonSchema for Tags {
    fn schema_name() -> Cow<'static, str> {
        "Tags".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "object",
            "description": "Free labels: a key holds a string, an integer, or a list of them.",
            "additionalProperties": {
                "anyOf": [
                    {"type": ["string", "integer"]},
                    {"type": "array", "items": {"type": ["string", "integer"]}},
                ],
            },
        })
    }
}
