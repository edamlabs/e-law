use std::fmt;

use jiff::civil::Date;
use schemars::JsonSchema;
use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use crate::glob::Glob;
use crate::invalid::{Invalid, Refusal};
use crate::reach::Reach;
use crate::selector::Selector;

const TOML_DATETIME: &str = "$__toml_private_datetime";

/// A declared place where a rule may break: a path, a symbol or both, with its reason and approver.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "ExceptionDocument")]
pub struct Exception {
    rule: Selector,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<Glob>,
    #[serde(skip_serializing_if = "Option::is_none")]
    symbol: Option<String>,
    reason: String,
    approved: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<String>")]
    expires: Option<Date>,
    reach: Reach,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("anyOf" = [{"required": ["path"]}, {"required": ["symbol"]}]))]
struct ExceptionDocument {
    rule: Selector,
    #[serde(default)]
    path: Option<Glob>,
    #[serde(default)]
    #[schemars(length(min = 1))]
    symbol: Option<String>,
    #[schemars(length(min = 1))]
    reason: String,
    #[schemars(length(min = 1))]
    approved: String,
    #[serde(default)]
    #[schemars(with = "Option<String>", extend("format" = "date"))]
    expires: Option<Expiry>,
    #[serde(default)]
    reach: Reach,
}

struct Expiry(Date);

struct ExpiryVisitor;

impl ExpiryVisitor {
    fn date<E: de::Error>(text: &str) -> Result<Expiry, E> {
        text.parse().map(Expiry).map_err(E::custom)
    }
}

impl<'de> Visitor<'de> for ExpiryVisitor {
    type Value = Expiry;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a date, `YYYY-MM-DD`")
    }

    fn visit_str<E: de::Error>(self, text: &str) -> Result<Expiry, E> {
        Self::date(text)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Expiry, A::Error> {
        match map.next_entry::<String, String>()? {
            Some((key, text)) if key == TOML_DATETIME => Self::date(&text),
            _ => Err(de::Error::invalid_type(de::Unexpected::Map, &self)),
        }
    }
}

impl<'de> Deserialize<'de> for Expiry {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(ExpiryVisitor)
    }
}

impl Exception {
    /// The rules it lets break.
    #[must_use]
    pub fn rule(&self) -> &Selector {
        &self.rule
    }

    /// The paths it covers.
    #[must_use]
    pub fn path(&self) -> Option<&Glob> {
        self.path.as_ref()
    }

    /// The symbol it covers.
    #[must_use]
    pub fn symbol(&self) -> Option<&str> {
        self.symbol.as_deref()
    }

    /// Why the rule may break there.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }

    /// Who approved it.
    #[must_use]
    pub fn approved(&self) -> &str {
        &self.approved
    }

    /// The day it stops silencing.
    #[must_use]
    pub fn expires(&self) -> Option<Date> {
        self.expires
    }

    /// Whether it drops findings or licenses pragmas.
    #[must_use]
    pub fn reach(&self) -> Reach {
        self.reach
    }
}

impl TryFrom<ExceptionDocument> for Exception {
    type Error = Invalid;

    fn try_from(document: ExceptionDocument) -> Result<Self, Self::Error> {
        let ExceptionDocument {
            rule,
            path,
            symbol,
            reason,
            approved,
            expires,
            reach,
        } = document;
        let blank = symbol.as_deref().is_some_and(str::is_empty);
        match (
            path.is_none() && symbol.is_none(),
            blank,
            reason.is_empty(),
            approved.is_empty(),
        ) {
            (true, ..) => Err(Invalid::new(
                Refusal::ExceptionWithoutTarget,
                format!("the exception to `{rule}` names neither a path nor a symbol"),
            )),
            (_, true, ..) => Err(Invalid::new(
                Refusal::EmptyText,
                format!("the exception to `{rule}` names an empty symbol"),
            )),
            (_, _, true, _) => Err(Invalid::new(
                Refusal::EmptyText,
                format!("the exception to `{rule}` has no reason"),
            )),
            (.., true) => Err(Invalid::new(
                Refusal::EmptyText,
                format!("the exception to `{rule}` has no approver"),
            )),
            _ => Ok(Self {
                rule,
                path,
                symbol,
                reason,
                approved,
                expires: expires.map(|Expiry(date)| date),
                reach,
            }),
        }
    }
}
