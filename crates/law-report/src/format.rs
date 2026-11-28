use std::fmt;
use std::str::FromStr;

/// How rules and trials are written.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Format {
    /// For a person, in lines.
    #[default]
    Text,
    /// As JSON.
    Json,
}

/// How a run's report is written.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Output {
    /// One line per finding, then the counts.
    #[default]
    Text,
    /// The report as JSON.
    Json,
    /// SARIF 2.1.0.
    Sarif,
    /// Markdown, for a pull request.
    Md,
}

/// How the coverage of the catalogues is written.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Table {
    /// Aligned columns.
    #[default]
    Text,
    /// A Markdown table.
    Md,
    /// The rows as JSON.
    Json,
}

/// A closed set of format names, chosen by the word that names one.
trait Named: Copy + 'static {
    const ALL: &'static [Self];

    fn name(self) -> &'static str;

    fn pick(text: &str) -> Result<Self, String> {
        Self::ALL
            .iter()
            .copied()
            .find(|format| format.name() == text)
            .ok_or_else(|| {
                let names: Vec<&str> = Self::ALL.iter().map(|format| format.name()).collect();
                format!("`{text}` is not one of {}", names.join(", "))
            })
    }
}

impl Named for Format {
    const ALL: &'static [Self] = &[Self::Text, Self::Json];

    fn name(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Json => "json",
        }
    }
}

impl Named for Output {
    const ALL: &'static [Self] = &[Self::Text, Self::Json, Self::Sarif, Self::Md];

    fn name(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Json => "json",
            Self::Sarif => "sarif",
            Self::Md => "md",
        }
    }
}

impl Named for Table {
    const ALL: &'static [Self] = &[Self::Text, Self::Md, Self::Json];

    fn name(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Md => "md",
            Self::Json => "json",
        }
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl fmt::Display for Output {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl fmt::Display for Table {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Format {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::pick(text)
    }
}

impl FromStr for Output {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::pick(text)
    }
}

impl FromStr for Table {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::pick(text)
    }
}
