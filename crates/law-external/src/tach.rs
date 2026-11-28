use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::sync::OnceLock;
use std::thread;

use law_config::{Loaded, Selection};
use law_engine::{BatchEngine, Sink};
use law_model::{RuleId, Severity, Span};

use crate::boundary::{TachCommand, TachDetails, TachDiagnostic, TachDocument, TachError, TachKind, TachSeverity};
use crate::fault::{Failure, Fault};
use crate::lines::Lines;
use crate::route::TachRoute;
use crate::tool::Tool;

const CHECK: [&str; 3] = ["check", "--output", "json"];
const EXTERNAL: [&str; 1] = ["check-external"];
const CONFIG: &str = "tach.toml";
const HERE: &str = ".";
const FOUND: [i32; 2] = [0, 1];
const PANICKED: &str = "reading a tach command panicked";
const LINELESS: &str = "tach gives no line";

type Placed<'a> = (u32, &'a TachKind, TachSeverity, bool);

/// The tach engine: one `tach check --output json` per run when a selected rule asks a kind it reports, one
/// `tach check-external` beside it when a selected rule asks a third-party kind, and `tach --version` beside both;
/// tach reads the whole project, its diagnostics kept within the paths of the run — those of the project, at
/// `tach.toml`'s first line, always — and signed by the rules they route to; tach offers no fix.
#[derive(Debug, Clone)]
pub struct TachEngine {
    path: PathBuf,
    version: OnceLock<String>,
    route: TachRoute,
    rules: Vec<RuleId>,
    commands: BTreeSet<TachCommand>,
}

impl TachEngine {
    fn fault(failure: Failure) -> law_engine::Fault {
        Fault {
            tool: Tool::Tach,
            failure,
        }
        .into()
    }

    fn exit(output: &Output) -> Failure {
        let text: Vec<u8> = output.stdout.iter().chain(&output.stderr).copied().collect();
        Failure::Exit {
            code: output.status.code(),
            stderr: Tool::head(&text),
        }
    }

    fn found(output: &Output) -> bool {
        output.status.code().is_some_and(|code| FOUND.contains(&code))
    }

    fn output(&self, root: &Path, args: &[&str]) -> Result<Output, Failure> {
        let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
        Tool::Tach.output(&self.path, root, &args)
    }

    fn checked(&self, root: &Path) -> Result<Vec<TachDiagnostic>, Failure> {
        let output = self.output(root, &CHECK)?;
        let document = serde_json::from_slice::<TachDocument>(&output.stdout)
            .ok()
            .filter(|_| Self::found(&output))
            .ok_or_else(|| Self::exit(&output))?;
        match document {
            TachDocument::Diagnostics(diagnostics) => Ok(diagnostics),
            TachDocument::Error(error) => error.diagnostics().ok_or(Failure::Refused { message: error.error }),
        }
    }

    fn external(&self, root: &Path) -> Result<Vec<TachDiagnostic>, Failure> {
        let output = self.output(root, &EXTERNAL)?;
        if !Self::found(&output) {
            return Err(Self::exit(&output));
        }
        let diagnostics = TachDiagnostic::external(&String::from_utf8_lossy(&output.stderr))
            .map_err(|line| Failure::Unknown { kind: line })?;
        let failed = !output.status.success();
        if failed && diagnostics.is_empty() {
            return Err(Self::exit(&output));
        }
        Ok(diagnostics)
    }

    fn diagnostics(&self, root: &Path, command: TachCommand) -> Result<Vec<TachDiagnostic>, Failure> {
        match command {
            TachCommand::Check => self.checked(root),
            TachCommand::External => self.external(root),
        }
    }

    fn within(paths: &[PathBuf], path: &Path) -> bool {
        paths
            .iter()
            .any(|start| start.as_os_str() == HERE || path.starts_with(start))
    }

    fn rating(severity: TachSeverity) -> Severity {
        match severity {
            TachSeverity::Error => Severity::Error,
            TachSeverity::Warning => Severity::Warning,
        }
    }

    fn sign(
        &self,
        kind: &TachKind,
        severity: TachSeverity,
        path: &Path,
        span: &Span,
        sink: &mut Sink,
        placed: bool,
    ) -> Option<Failure> {
        let Some(line) = kind.line() else {
            return Some(Failure::Unknown {
                kind: kind.name.clone(),
            });
        };
        let line = if placed { line } else { format!("{line}: {LINELESS}") };
        let rating = Self::rating(severity);
        self.route.route(&kind.name).into_iter().for_each(|rule| {
            sink.report(rule, path, span.clone(), Some(&line), None);
            sink.findings
                .last_mut()
                .into_iter()
                .for_each(|finding| finding.severity = finding.severity.max(rating));
        });
        None
    }

    fn file(&self, root: &Path, path: &Path, placed: &[Placed<'_>], sink: &mut Sink) -> Vec<Failure> {
        let text = match fs::read_to_string(root.join(path)) {
            Ok(text) => text,
            Err(error) => return vec![Failure::io(path, &error)],
        };
        let lines = Lines::new(&text);
        placed
            .iter()
            .filter_map(|(line, kind, severity, at)| match lines.line(*line) {
                Some(span) => self.sign(kind, *severity, path, &span, sink, *at),
                None => Some(Failure::Place {
                    path: path.to_owned(),
                    line: *line,
                    column: 1,
                }),
            })
            .collect()
    }

    fn configured(kind: &TachKind, severity: TachSeverity) -> Option<Failure> {
        match (kind.configuration(), severity) {
            (false, _) => Some(Failure::Unknown {
                kind: kind.name.clone(),
            }),
            (true, TachSeverity::Error) => kind.line().map(|message| Failure::Refused { message }),
            (true, TachSeverity::Warning) => None,
        }
    }

    fn code(diagnostic: &TachDiagnostic) -> Option<&str> {
        match diagnostic.details() {
            TachDetails::Code(kind) => Some(kind.name.as_str()),
            TachDetails::Configuration(_) => None,
        }
    }

    fn spot(diagnostic: &TachDiagnostic) -> Option<(&Path, &str, bool)> {
        match diagnostic {
            TachDiagnostic::Located(located) => Self::code(diagnostic).map(|code| {
                (
                    located.file_path.as_path(),
                    code,
                    located.original_line_number.is_some(),
                )
            }),
            TachDiagnostic::Global(_) => None,
        }
    }

    fn twinless(diagnostics: &[TachDiagnostic]) -> Vec<&TachDiagnostic> {
        let placed: BTreeSet<(&Path, &str)> = diagnostics
            .iter()
            .filter_map(Self::spot)
            .filter_map(|(path, code, placed)| placed.then_some((path, code)))
            .collect();
        diagnostics
            .iter()
            .filter(|diagnostic| {
                Self::spot(diagnostic).is_none_or(|(path, code, at)| at || !placed.contains(&(path, code)))
            })
            .collect()
    }

    fn echoed(failure: &Failure, codes: &BTreeSet<&str>) -> bool {
        match failure {
            Failure::Exit { stderr, .. } => TachError::refusal(stderr).is_some_and(|kind| codes.contains(kind)),
            _ => false,
        }
    }

    fn report(&self, root: &Path, paths: &[PathBuf], diagnostics: &[&TachDiagnostic], sink: &mut Sink) {
        let config = PathBuf::from(CONFIG);
        let mut files: BTreeMap<&Path, Vec<Placed<'_>>> = BTreeMap::new();
        let unplaced: Vec<Failure> = diagnostics
            .iter()
            .filter_map(|diagnostic| match (diagnostic, diagnostic.details()) {
                (_, TachDetails::Configuration(kind)) => Self::configured(kind, diagnostic.severity()),
                (TachDiagnostic::Global(_), TachDetails::Code(kind)) => {
                    self.sign(kind, diagnostic.severity(), &config, &Sink::origin(), sink, true)
                }
                (TachDiagnostic::Located(located), TachDetails::Code(kind)) => {
                    if Self::within(paths, &located.file_path) {
                        files.entry(&located.file_path).or_default().push((
                            located.line_number,
                            kind,
                            located.severity,
                            located.original_line_number.is_some(),
                        ));
                    }
                    None
                }
            })
            .collect();
        let unread: Vec<Failure> = files
            .iter()
            .flat_map(|(path, placed)| self.file(root, path, placed, sink))
            .collect();
        sink.faults.extend(unplaced.into_iter().chain(unread).map(Self::fault));
    }

    /// The engine of the selected tach rules of what was loaded; none when no tach rule is selected. Its version is
    /// asked while it checks, beside its commands.
    ///
    /// # Errors
    /// When a selected rule names a kind e-law does not know, or tach is nowhere it is looked for.
    pub fn new(loaded: &Loaded, selection: &Selection) -> Result<Option<Self>, Fault> {
        let route = TachRoute::new(
            loaded.catalogues.iter().flat_map(|catalogue| &catalogue.rules),
            |rule| selection.admits(rule),
        );
        let rules: Vec<RuleId> = route.rules().iter().map(|rule| rule.id.clone()).collect();
        if rules.is_empty() {
            return Ok(None);
        }
        let fault = |failure| Fault {
            tool: Tool::Tach,
            failure,
        };
        let commands = route
            .codes()
            .into_iter()
            .map(|code| match code {
                Some(code) => TachCommand::of(code)
                    .map(|command| vec![command])
                    .ok_or_else(|| fault(Failure::Unknown { kind: code.to_owned() })),
                None => Ok(vec![TachCommand::Check, TachCommand::External]),
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect();
        let root = &loaded.project.root;
        let path = Tool::Tach
            .locate(root)
            .ok_or_else(|| fault(Failure::Missing { rules: rules.clone() }))?;
        Ok(Some(Self {
            path,
            version: OnceLock::new(),
            route,
            rules,
            commands,
        }))
    }
}

impl BatchEngine for TachEngine {
    fn name(&self) -> &str {
        Tool::Tach.name()
    }

    fn version(&self) -> &str {
        self.version.get().map_or("", String::as_str)
    }

    fn rules(&self) -> &[RuleId] {
        &self.rules
    }

    fn check(&self, root: &Path, paths: &[PathBuf], sink: &mut Sink) {
        let (version, results): (_, Vec<Result<Vec<TachDiagnostic>, Failure>>) = thread::scope(|scope| {
            let version = self
                .version
                .get()
                .is_none()
                .then(|| scope.spawn(|| Tool::Tach.version(&self.path, root)));
            let running: Vec<_> = self
                .commands
                .iter()
                .map(|command| scope.spawn(move || self.diagnostics(root, *command)))
                .collect();
            let results = running
                .into_iter()
                .map(|running| {
                    running.join().unwrap_or_else(|_| {
                        Err(Failure::Refused {
                            message: PANICKED.to_owned(),
                        })
                    })
                })
                .collect();
            let version = version.map(|version| {
                version.join().unwrap_or_else(|_| {
                    Err(Failure::Refused {
                        message: PANICKED.to_owned(),
                    })
                })
            });
            (version, results)
        });
        match version {
            Some(Ok(version)) => drop(self.version.set(version)),
            Some(Err(failure)) => sink.faults.push(Self::fault(failure)),
            None => {}
        }
        let (found, failed): (Vec<_>, Vec<_>) = results.into_iter().partition(Result::is_ok);
        let diagnostics: Vec<TachDiagnostic> = found.into_iter().flatten().flatten().collect();
        let codes: BTreeSet<&str> = diagnostics.iter().filter_map(Self::code).collect();
        sink.faults.extend(
            failed
                .into_iter()
                .filter_map(Result::err)
                .filter(|failure| !Self::echoed(failure, &codes))
                .map(Self::fault),
        );
        self.report(root, paths, &Self::twinless(&diagnostics), sink);
    }
}
