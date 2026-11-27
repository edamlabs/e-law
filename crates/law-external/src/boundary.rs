use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;
use serde_json::Value;

const CYCLE: &str = "Circular dependency";
const VISIBILITY: &str = "Visibility error";
const CIRCULAR: &str = "CircularDependency";
const VISIBLE: &str = "Visibility";
const REFUSALS: [(&str, &str); 2] = [("Circular dependency error", CIRCULAR), ("Visibility error", VISIBLE)];
const SEPARATOR: &str = ", ";
const FAIL: &str = "[FAIL] ";
const WARN: &str = "[WARN] ";
const GLOBAL: [&str; 2] = ["Error: ", "Warning: "];
const PLACE: &str = ": ";
const OPEN: char = '{';
const CLOSE: char = '}';
const TEXT: [(&str, &str); 6] = [
    (
        "UndeclaredExternalDependency",
        "Dependency '{dependency}' is not declared in package '{package_name}'.",
    ),
    (
        "UnusedExternalDependency",
        "External package '{package_module_name}' is not used in package '{package_name}'.",
    ),
    (
        "ModuleUndeclaredExternalDependency",
        "Module '{usage_module}' does not declare a dependency on external package '{dependency}'.",
    ),
    (
        "ModuleForbiddenExternalDependency",
        "Module '{usage_module}' cannot depend on external package '{dependency}'.",
    ),
    ("UnusedIgnoreDirective", "Ignore directive is unused."),
    ("MissingIgnoreDirectiveReason", "Ignore directive is missing a reason."),
];
const CODES: [(&str, TachCommand, &str); 16] = [
    (
        "UndeclaredDependency",
        TachCommand::Check,
        "{usage_module} imports {dependency}, undeclared",
    ),
    (
        "ForbiddenDependency",
        TachCommand::Check,
        "{usage_module} imports {dependency}, forbidden",
    ),
    (
        "DeprecatedDependency",
        TachCommand::Check,
        "{usage_module} imports {dependency}, deprecated",
    ),
    (
        "LayerViolation",
        TachCommand::Check,
        "{usage_module} ({usage_layer}) imports {dependency} ({definition_layer}), a higher layer",
    ),
    (
        "ClosedLayerViolation",
        TachCommand::Check,
        "{usage_module} ({usage_layer}) imports {dependency} ({definition_layer}) across the closed layer {closed_layer}",
    ),
    (
        "PrivateDependency",
        TachCommand::Check,
        "{usage_module} imports {dependency}, outside the interface of {definition_module}",
    ),
    (
        "InvalidDataTypeExport",
        TachCommand::Check,
        "{usage_module} imports {dependency}, not of the data type {expected_data_type}",
    ),
    (
        "UnusedIgnoreDirective",
        TachCommand::Check,
        "a tach-ignore that ignores nothing",
    ),
    (
        "MissingIgnoreDirectiveReason",
        TachCommand::Check,
        "a tach-ignore with no reason",
    ),
    (
        "UnnecessarilyIgnoredDependency",
        TachCommand::Check,
        "{dependency} is ignored, yet allowed",
    ),
    (CIRCULAR, TachCommand::Check, "{modules} depend on each other"),
    (
        VISIBLE,
        TachCommand::Check,
        "{dependent} depends on {dependency}, outside its visibility ({visibility})",
    ),
    (
        "UndeclaredExternalDependency",
        TachCommand::External,
        "{dependency} is imported, undeclared in {package_name}",
    ),
    (
        "UnusedExternalDependency",
        TachCommand::External,
        "{package_module_name} is declared in {package_name}, never imported",
    ),
    (
        "ModuleUndeclaredExternalDependency",
        TachCommand::External,
        "{usage_module} imports {dependency}, undeclared",
    ),
    (
        "ModuleForbiddenExternalDependency",
        TachCommand::External,
        "{usage_module} imports {dependency}, forbidden",
    ),
];
const CONFIGURATIONS: [&str; 9] = [
    "ModuleConfigNotFound",
    "ModuleNotFound",
    "NoChecksEnabled",
    "NoFirstPartyImportsFound",
    "SkippedFileIoError",
    "SkippedFileSyntaxError",
    "SkippedPyProjectParsingError",
    "SkippedUnknownError",
    "UnknownLayer",
];

/// The tach command that reports a kind of diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TachCommand {
    /// `tach check --output json`: the boundaries between first-party modules.
    Check,
    /// `tach check-external`: the third-party dependencies, written as text only.
    External,
}

/// What `tach check --output json` writes: its diagnostics, or the error that stopped it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum TachDocument {
    /// The diagnostics, none for a clean project.
    Diagnostics(Vec<TachDiagnostic>),
    /// The error that stopped the check: a cycle, a visibility error, or any other.
    Error(TachError),
}

/// The error object tach writes in place of its diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TachError {
    /// What it says: `Circular dependency`, `Visibility error`, or anything else.
    pub error: String,
    /// The modules of a cycle.
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Each visibility error: the dependent module, its dependency, and the dependency's visibility.
    #[serde(default)]
    pub visibility_errors: Vec<(String, String, Vec<String>)>,
}

/// One tach diagnostic: at a line of a file, or of the project.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub enum TachDiagnostic {
    /// At a line of a file.
    Located(TachLocated),
    /// Of the whole project.
    Global(TachGlobal),
}

/// A diagnostic at a line of a file.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TachLocated {
    /// The file, from the project root.
    pub file_path: PathBuf,
    /// The 1-based line.
    pub line_number: u32,
    /// The line tach read it at; none when it placed it at line 1 for want of one.
    #[serde(default)]
    pub original_line_number: Option<u32>,
    /// How tach rates it.
    pub severity: TachSeverity,
    /// What it is.
    pub details: TachDetails,
}

/// A diagnostic of the whole project.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TachGlobal {
    /// How tach rates it.
    pub severity: TachSeverity,
    /// What it is.
    pub details: TachDetails,
}

/// How tach rates a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum TachSeverity {
    /// It fails the check.
    Error,
    /// It does not.
    Warning,
}

/// What a diagnostic is: a breach of the code, or of the configuration.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub enum TachDetails {
    /// A breach in the code: an import, a directive.
    Code(TachKind),
    /// A configuration tach reports wrong or empty.
    Configuration(TachKind),
}

/// A kind of diagnostic and its fields: `{"UndeclaredDependency": {"dependency": …}}`, `{"UnusedIgnoreDirective": []}`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "BTreeMap<String, Fields>")]
pub struct TachKind {
    /// tach's name for the kind.
    pub name: String,
    /// Its fields, each rendered as text.
    pub fields: BTreeMap<String, String>,
}

/// A kind's fields as tach writes them: an object, or `[]` for a kind with none.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
enum Fields {
    Named(BTreeMap<String, Value>),
    Unit(Vec<Value>),
}

impl TachCommand {
    /// The command that reports a kind tach names; none for a kind e-law does not know.
    #[must_use]
    pub fn of(kind: &str) -> Option<Self> {
        CODES
            .iter()
            .find(|(name, _, _)| *name == kind)
            .map(|(_, command, _)| *command)
    }
}

impl TachError {
    fn global(name: &str, fields: BTreeMap<String, String>) -> TachDiagnostic {
        TachDiagnostic::Global(TachGlobal {
            severity: TachSeverity::Error,
            details: TachDetails::Code(TachKind {
                name: name.to_owned(),
                fields,
            }),
        })
    }

    fn visibility(&self) -> Vec<TachDiagnostic> {
        self.visibility_errors
            .iter()
            .map(|(dependent, dependency, visibility)| {
                let fields = BTreeMap::from([
                    ("dependent".to_owned(), dependent.clone()),
                    ("dependency".to_owned(), dependency.clone()),
                    ("visibility".to_owned(), visibility.join(SEPARATOR)),
                ]);
                Self::global(VISIBLE, fields)
            })
            .collect()
    }

    /// The kind of code diagnostic a refusal of `tach check-external` stands for: `CircularDependency` for
    /// `Circular dependency error`, `Visibility` for `Visibility error`; none for any other text.
    #[must_use]
    pub fn refusal(text: &str) -> Option<&'static str> {
        REFUSALS
            .iter()
            .find(|(said, _)| *said == text.trim())
            .map(|(_, kind)| *kind)
    }

    /// The diagnostics an error stands for: one `CircularDependency` for a cycle, one `Visibility` per visibility
    /// error; none for any other error, which is tach refusing the project.
    #[must_use]
    pub fn diagnostics(&self) -> Option<Vec<TachDiagnostic>> {
        match self.error.as_str() {
            CYCLE => Some(vec![Self::global(
                CIRCULAR,
                BTreeMap::from([("modules".to_owned(), self.dependencies.join(SEPARATOR))]),
            )]),
            VISIBILITY => Some(self.visibility()),
            _ => None,
        }
    }
}

impl TachDiagnostic {
    fn located(text: &str) -> Option<(PathBuf, u32, &str)> {
        text.match_indices(PLACE).find_map(|(at, _)| {
            let (file, line) = text[..at].rsplit_once(':')?;
            let line = line.parse().ok()?;
            Some((PathBuf::from(file), line, &text[at + PLACE.len()..]))
        })
    }

    fn filled(template: &str, text: &str) -> Option<BTreeMap<String, String>> {
        let mut parts = template.split(OPEN);
        let rest = text.strip_prefix(parts.next()?)?;
        let (fields, rest) = parts.try_fold((BTreeMap::new(), rest), |(mut fields, rest), part| {
            let (name, literal) = part.split_once(CLOSE)?;
            let (value, after) = if literal.is_empty() {
                (rest, "")
            } else {
                let at = rest.find(literal)?;
                (&rest[..at], &rest[at + literal.len()..])
            };
            fields.insert(name.to_owned(), value.to_owned());
            Some((fields, after))
        })?;
        rest.is_empty().then_some(fields)
    }

    fn kind(message: &str) -> Option<TachKind> {
        TEXT.iter().find_map(|(name, template)| {
            Self::filled(template, message).map(|fields| TachKind {
                name: (*name).to_owned(),
                fields,
            })
        })
    }

    fn line(line: &str) -> Option<Result<Self, String>> {
        let (severity, rest) = line
            .strip_prefix(FAIL)
            .map(|rest| (TachSeverity::Error, rest))
            .or_else(|| line.strip_prefix(WARN).map(|rest| (TachSeverity::Warning, rest)))?;
        let global = GLOBAL.iter().find_map(|head| rest.strip_prefix(head));
        let diagnostic = match global {
            Some(message) => Self::kind(message).map(|kind| {
                Self::Global(TachGlobal {
                    severity,
                    details: TachDetails::Code(kind),
                })
            }),
            None => Self::located(rest).and_then(|(file_path, line_number, message)| {
                Self::kind(message).map(|kind| {
                    Self::Located(TachLocated {
                        file_path,
                        line_number,
                        original_line_number: Some(line_number),
                        severity,
                        details: TachDetails::Code(kind),
                    })
                })
            }),
        };
        Some(diagnostic.ok_or_else(|| line.to_owned()))
    }

    /// The diagnostics of what `tach check-external` writes on stderr, one per `[FAIL]` or `[WARN]` line, the rest of
    /// its text ignored.
    ///
    /// # Errors
    /// The first such line that is no kind e-law knows.
    pub fn external(text: &str) -> Result<Vec<Self>, String> {
        text.lines().filter_map(Self::line).collect()
    }

    /// What it is.
    #[must_use]
    pub fn details(&self) -> &TachDetails {
        match self {
            Self::Located(located) => &located.details,
            Self::Global(global) => &global.details,
        }
    }

    /// How tach rates it.
    #[must_use]
    pub fn severity(&self) -> TachSeverity {
        match self {
            Self::Located(located) => located.severity,
            Self::Global(global) => global.severity,
        }
    }
}

impl TachKind {
    fn render(template: &str, fields: &BTreeMap<String, String>) -> String {
        let mut parts = template.split(OPEN);
        let head = parts.next().unwrap_or_default().to_owned();
        parts.fold(head, |mut out, part| {
            let (name, literal) = part.split_once(CLOSE).unwrap_or((part, ""));
            out.push_str(fields.get(name).map_or(name, String::as_str));
            out.push_str(literal);
            out
        })
    }

    /// Whether e-law knows it as a kind of configuration diagnostic.
    #[must_use]
    pub fn configuration(&self) -> bool {
        CONFIGURATIONS.contains(&self.name.as_str())
    }

    /// One line of its fields: `pkg.core imports pkg.ui.view, undeclared`; for a configuration kind, its name and
    /// fields; none for a code kind e-law does not know.
    #[must_use]
    pub fn line(&self) -> Option<String> {
        let fields = || {
            self.fields
                .iter()
                .map(|(name, value)| format!("{name} {value}"))
                .collect::<Vec<_>>()
        };
        if self.configuration() {
            return Some(
                [self.name.clone()]
                    .into_iter()
                    .chain(fields())
                    .collect::<Vec<_>>()
                    .join(SEPARATOR),
            );
        }
        CODES
            .iter()
            .find(|(name, _, _)| *name == self.name)
            .map(|(_, _, template)| Self::render(template, &self.fields))
    }
}

impl TryFrom<BTreeMap<String, Fields>> for TachKind {
    type Error = String;

    fn try_from(map: BTreeMap<String, Fields>) -> Result<Self, Self::Error> {
        let count = map.len();
        let mut entries = map.into_iter();
        match (entries.next(), count) {
            (Some((name, fields)), 1) => Ok(Self {
                name,
                fields: match fields {
                    Fields::Named(fields) => fields
                        .into_iter()
                        .map(|(key, value)| {
                            let text = value.as_str().map_or_else(|| value.to_string(), str::to_owned);
                            (key, text)
                        })
                        .collect(),
                    Fields::Unit(_) => BTreeMap::new(),
                },
            }),
            _ => Err(format!("a kind is one name with its fields, not {count}")),
        }
    }
}
