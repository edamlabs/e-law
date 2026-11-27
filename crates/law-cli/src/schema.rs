use law_config::{Schemas, schema};
use serde_json::{Map, Value};

use crate::cli::{SchemaArgs, SchemaName};
use crate::exit::Exit;

const ALL: [SchemaName; 5] = [
    SchemaName::Config,
    SchemaName::Rules,
    SchemaName::Profiles,
    SchemaName::Exceptions,
    SchemaName::Report,
];

impl SchemaName {
    fn word(self) -> &'static str {
        match self {
            Self::Config => "config",
            Self::Rules => "rules",
            Self::Profiles => "profiles",
            Self::Exceptions => "exceptions",
            Self::Report => "report",
        }
    }

    fn pick(self, schemas: &Schemas) -> &Value {
        match self {
            Self::Config => schemas.config.as_value(),
            Self::Rules => schemas.rules.as_value(),
            Self::Profiles => schemas.profiles.as_value(),
            Self::Exceptions => schemas.exceptions.as_value(),
            Self::Report => schemas.report.as_value(),
        }
    }
}

impl SchemaArgs {
    /// The schema named, or all five keyed by name, as JSON on stdout; no project is needed.
    #[must_use]
    pub fn run(&self) -> Exit {
        let schemas = schema();
        let value = self.name.map_or_else(
            || {
                let all: Map<String, Value> = ALL
                    .into_iter()
                    .map(|name| (name.word().to_owned(), name.pick(&schemas).clone()))
                    .collect();
                Value::Object(all)
            },
            |name| name.pick(&schemas).clone(),
        );
        Exit::emit(|out| {
            serde_json::to_writer_pretty(&mut *out, &value)?;
            writeln!(out)
        })
        .map_or_else(|exit| exit, |()| Exit::Clean)
    }
}
