use std::io::{self, Write};
use std::path::{Path, PathBuf};

use law_model::{
    Case, Check, FixPolicy, FixedCase, Former, Glob, GraphSpec, Kind, MembersSpec, MetricSpec, ParitySpec, ProfileSpec,
    RuffSpec, Rule, Severity, ShapeSpec, Snippet, TachSpec, TreeSpec, TySpec,
};

use crate::format::Format;

const NONE: &str = "none";
const INDENT: &str = "    ";
const DEEPER: &str = "      ";
const LIST: &str = ", ";
const FILES: &str = "files";

struct Card;

impl Card {
    fn fix(policy: &FixPolicy) -> &'static str {
        match policy {
            FixPolicy::Safe => "safe",
            FixPolicy::Unsafe => "unsafe",
            FixPolicy::None { .. } => NONE,
        }
    }

    fn policy(policy: &FixPolicy) -> String {
        match policy {
            FixPolicy::None { reason } => format!("{NONE} ({reason})"),
            other => Self::fix(other).to_owned(),
        }
    }

    fn engine(check: &Check) -> String {
        match check {
            Check::Ruff(RuffSpec {
                code,
                message,
                settings,
            }) => {
                let keys: Vec<&str> = settings.keys().map(String::as_str).collect();
                [
                    Some(format!("ruff {code}")),
                    message.as_ref().map(|message| format!("/{message}/")),
                    (!keys.is_empty()).then(|| format!("with {}", keys.join(LIST))),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<String>>()
                .join(" ")
            }
            Check::Ty(TySpec { rule }) => rule
                .as_ref()
                .map_or_else(|| "ty".to_owned(), |rule| format!("ty {rule}")),
            Check::Tach(TachSpec { code }) => code
                .as_ref()
                .map_or_else(|| "tach".to_owned(), |code| format!("tach {code}")),
            Check::Profile(ProfileSpec { kinds, aspect }) => {
                let kinds: Vec<&str> = kinds.iter().map(Kind::as_str).collect();
                format!("profile {} {aspect}", kinds.join(LIST))
            }
            Check::Metric(MetricSpec { kinds, measure, over }) => {
                let kinds: Vec<&str> = kinds.iter().map(Kind::as_str).collect();
                format!("metric {} {measure} over {over}", kinds.join(LIST))
            }
            Check::Members(MembersSpec { lineage, order, .. }) => {
                let parts = [lineage.as_ref().map(|_| "lineage"), order.as_ref().map(|_| "order")];
                Self::parts(check, parts)
            }
            Check::Shape(ShapeSpec {
                classes,
                functions,
                constants,
            }) => {
                let parts = [
                    classes.as_ref().map(|_| "classes"),
                    functions.as_ref().map(|_| "functions"),
                    constants.as_ref().map(|_| "constants"),
                ];
                Self::parts(check, parts)
            }
            Check::Tree(TreeSpec { dirs, .. }) => Self::parts(check, dirs.iter().map(|dir| Some(dir.as_str()))),
            Check::Graph(GraphSpec {
                aspect,
                entries,
                typing,
            }) => {
                let entries = entries.iter().map(Glob::as_str).collect::<Vec<&str>>().join(LIST);
                [
                    Some(format!("graph {aspect}")),
                    typing.then(|| "typing".to_owned()),
                    (!entries.is_empty()).then(|| format!("entries {entries}")),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<String>>()
                .join(" ")
            }
            Check::Parity(ParitySpec {
                names,
                entries,
                relation,
            }) => {
                let from = entries
                    .glob
                    .as_ref()
                    .map_or_else(|| "beside".to_owned(), |glob| format!("glob {}", glob.as_str()));
                format!("parity {} of {} {from}, {relation}", names.of, names.class)
            }
            other => other.name().to_owned(),
        }
    }

    fn parts<'a>(check: &Check, parts: impl IntoIterator<Item = Option<&'a str>>) -> String {
        let parts: Vec<&str> = parts.into_iter().flatten().collect();
        format!("{} {}", check.name(), parts.join(LIST))
    }

    fn tags(rule: &Rule) -> String {
        let tags: String = rule
            .tags
            .iter()
            .flat_map(|(key, values)| values.map(move |value| format!(" {key}={value}")))
            .collect();
        if tags.is_empty() { format!(" {NONE}") } else { tags }
    }

    fn scope(rule: &Rule) -> String {
        let globs = |globs: &[Glob]| globs.iter().map(Glob::as_str).collect::<Vec<_>>().join(" ");
        let scope = &rule.scope;
        [
            Some(format!("include {}", globs(&scope.include))),
            (!scope.exclude.is_empty()).then(|| format!("exclude {}", globs(&scope.exclude))),
            scope.kind.as_ref().map(|kind| format!("kind {kind}")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("; ")
    }

    fn code(text: &str, indent: &str, out: &mut dyn Write) -> io::Result<()> {
        text.lines().try_for_each(|line| writeln!(out, "{indent}{line}"))
    }

    fn place(rule: &Rule, path: Option<&PathBuf>) -> PathBuf {
        path.cloned().unwrap_or_else(|| rule.scope.sample())
    }

    fn head(name: &str, empty: bool, out: &mut dyn Write) -> io::Result<()> {
        if empty {
            writeln!(out, "{name}: {NONE}")
        } else {
            writeln!(out, "{name}:")
        }
    }

    fn cases(rule: &Rule, name: &str, cases: &[Case], out: &mut dyn Write) -> io::Result<()> {
        Self::head(name, cases.is_empty(), out)?;
        cases
            .iter()
            .enumerate()
            .try_for_each(|(index, case)| match &case.snippet {
                Snippet::Code(code) => {
                    Self::entry(index, &Self::place(rule, case.path.as_ref()), out)?;
                    Self::code(code, INDENT, out)
                }
                Snippet::Files(files) => {
                    Self::entry(index, Path::new(FILES), out)?;
                    files.iter().try_for_each(|(path, code)| {
                        writeln!(out, "{INDENT}{}:", path.display())?;
                        Self::code(code, DEEPER, out)
                    })
                }
            })
    }

    fn fixed(rule: &Rule, cases: &[FixedCase], out: &mut dyn Write) -> io::Result<()> {
        Self::head("fixed", cases.is_empty(), out)?;
        cases.iter().enumerate().try_for_each(|(index, case)| {
            Self::entry(index, &Self::place(rule, case.path.as_ref()), out)?;
            writeln!(out, "{INDENT}code:")?;
            Self::code(&case.code, DEEPER, out)?;
            writeln!(out, "{INDENT}expect:")?;
            Self::code(&case.expect, DEEPER, out)
        })
    }

    fn entry(index: usize, path: &Path, out: &mut dyn Write) -> io::Result<()> {
        writeln!(out, "  {}. {}", index + 1, path.display())
    }

    fn text(rule: &Rule, out: &mut dyn Write) -> io::Result<()> {
        writeln!(out, "id: {}", rule.id)?;
        Self::formerly(rule, out)?;
        writeln!(out, "message: {}", rule.message.as_str())?;
        writeln!(out, "tags:{}", Self::tags(rule))?;
        writeln!(out, "severity: {}", Self::severity(rule))?;
        writeln!(out, "engine: {}", Self::engine(&rule.check))?;
        writeln!(out, "fix: {}", Self::policy(&rule.fix))?;
        writeln!(out, "scope: {}", Self::scope(rule))?;
        Self::cases(rule, "valid", &rule.cases.valid, out)?;
        Self::cases(rule, "invalid", &rule.cases.invalid, out)?;
        Self::fixed(rule, &rule.cases.fixed, out)
    }

    fn formerly(rule: &Rule, out: &mut dyn Write) -> io::Result<()> {
        let former: Vec<&str> = rule.formerly.iter().map(Former::as_str).collect();
        match former.as_slice() {
            [] => Ok(()),
            _ => writeln!(out, "formerly: {}", former.join(LIST)),
        }
    }

    fn severity(rule: &Rule) -> &'static str {
        match rule.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

/// One rule written in a format: in text, its fields then its cases; in JSON, the rule.
///
/// # Errors
/// When the output cannot be written.
pub fn rule(rule: &Rule, format: Format, out: &mut dyn Write) -> io::Result<()> {
    match format {
        Format::Text => Card::text(rule, out),
        Format::Json => {
            serde_json::to_writer_pretty(&mut *out, rule)?;
            writeln!(out)
        }
    }
}

/// Rules written in a format: in text, one line each, `id  [fix]  message`; in JSON, the list.
///
/// # Errors
/// When the output cannot be written.
pub fn rules(rules: &[&Rule], format: Format, out: &mut dyn Write) -> io::Result<()> {
    match format {
        Format::Text => rules.iter().try_for_each(|rule| {
            writeln!(
                out,
                "{}  [{}]  {}",
                rule.id,
                Card::fix(&rule.fix),
                rule.message.as_str()
            )
        }),
        Format::Json => {
            serde_json::to_writer_pretty(&mut *out, rules)?;
            writeln!(out)
        }
    }
}
