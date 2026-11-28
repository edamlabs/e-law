use law_model::{Profile, Report, Rule};
use schemars::generate::SchemaSettings;
use schemars::{JsonSchema, Schema};

use crate::config::Config;
use crate::exceptions::Exceptions;

/// The JSON Schemas (2020-12) of every file the tool reads or writes.
#[derive(Debug, Clone, PartialEq)]
pub struct Schemas {
    /// `law.toml`, or `[tool.law]`.
    pub config: Schema,
    /// A catalogue's rule file: a list of rules.
    pub rules: Schema,
    /// A catalogue's profile file: a list of profiles.
    pub profiles: Schema,
    /// An exceptions file.
    pub exceptions: Schema,
    /// A run's report.
    pub report: Schema,
}

fn of<T: JsonSchema>() -> Schema {
    SchemaSettings::draft2020_12()
        .for_deserialize()
        .into_generator()
        .into_root_schema_for::<T>()
}

/// The JSON Schemas of the configuration, the rule and profile lists, the exceptions and the report.
#[must_use]
pub fn schema() -> Schemas {
    Schemas {
        config: of::<Config>(),
        rules: of::<Vec<Rule>>(),
        profiles: of::<Vec<Profile>>(),
        exceptions: of::<Exceptions>(),
        report: of::<Report>(),
    }
}
