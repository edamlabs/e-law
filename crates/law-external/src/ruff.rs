use std::collections::BTreeMap;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};

use law_config::{Loaded, Selection};
use law_engine::{BatchEngine, CaseEngine, Sink};
use law_model::{Code, Edit, Fix, Rule, RuleId};

use crate::diagnostic::{RuffDiagnostic, RuffFix};
use crate::fault::{Failure, Fault};
use crate::lines::Lines;
use crate::route::RuffRoute;
use crate::tool::Tool;

const ARGS: [&str; 6] = [
    "check",
    "--output-format",
    "json",
    "--no-fix",
    "--exit-zero",
    "--select",
];
const END: &str = "--";
const COMMA: &str = ",";
const CONFIGS: [&str; 3] = [".ruff.toml", "ruff.toml", "pyproject.toml"];

/// The ruff engine: one `ruff check` per run over the codes its selected rules name, each diagnostic signed by the
/// rules it routes to, ruff's fix carried at the weaker of ruff's and the rule's safety.
#[derive(Debug, Clone)]
pub struct RuffEngine {
    path: PathBuf,
    version: String,
    project: PathBuf,
    route: RuffRoute,
    rules: Vec<RuleId>,
}

impl RuffEngine {
    fn fault(failure: Failure) -> law_engine::Fault {
        Fault {
            tool: Tool::Ruff,
            failure,
        }
        .into()
    }

    fn diagnostics(&self, cwd: &Path, codes: &[&Code], paths: &[PathBuf]) -> Result<Vec<RuffDiagnostic>, Failure> {
        let select = codes.iter().map(|code| code.as_str()).collect::<Vec<_>>().join(COMMA);
        let args: Vec<OsString> = ARGS
            .iter()
            .map(OsString::from)
            .chain([OsString::from(select), OsString::from(END)])
            .chain(paths.iter().map(OsString::from))
            .collect();
        let args: Vec<&OsStr> = args.iter().map(OsString::as_os_str).collect();
        let out = Tool::Ruff.run(&self.path, cwd, &args)?;
        serde_json::from_slice(&out).map_err(|error| Failure::Document {
            message: error.to_string(),
        })
    }

    fn fix(rule: &Rule, path: &Path, lines: &Lines<'_>, fix: &RuffFix) -> Result<Option<Fix>, Failure> {
        let Some(safety) = fix.applicability.safety(&rule.fix) else {
            return Ok(None);
        };
        let edits = fix
            .edits
            .iter()
            .map(|edit| {
                lines
                    .span(edit.location.position(), edit.end_location.position())
                    .map(|span| Edit::Replace {
                        path: path.to_owned(),
                        bytes: span.bytes,
                        text: edit.content.clone(),
                    })
                    .ok_or(Failure::Place {
                        path: path.to_owned(),
                        line: edit.location.row,
                        column: edit.location.column,
                    })
            })
            .collect::<Result<Vec<Edit>, Failure>>()?;
        Ok(Some(Fix { safety, edits }))
    }

    fn signed(
        &self,
        path: &Path,
        lines: &Lines<'_>,
        diagnostic: &RuffDiagnostic,
        only: Option<&RuleId>,
        sink: &mut Sink,
    ) -> Result<(), Failure> {
        let place = Failure::Place {
            path: path.to_owned(),
            line: diagnostic.location.row,
            column: diagnostic.location.column,
        };
        let span = lines
            .span(diagnostic.location.position(), diagnostic.end_location.position())
            .ok_or(place)?;
        self.route
            .route(&diagnostic.code, &diagnostic.message)
            .into_iter()
            .filter(|rule| only.is_none_or(|only| &rule.id == only))
            .try_for_each(|rule| {
                let fix = diagnostic
                    .fix
                    .as_ref()
                    .map(|fix| Self::fix(rule, path, lines, fix))
                    .transpose()?
                    .flatten();
                sink.report(rule, path, span.clone(), Some(&diagnostic.message), fix);
                Ok(())
            })
    }

    fn file(
        &self,
        root: &Path,
        path: &Path,
        diagnostics: &[&RuffDiagnostic],
        only: Option<&RuleId>,
        sink: &mut Sink,
    ) -> Result<(), Failure> {
        let text = fs::read_to_string(root.join(path)).map_err(|error| Failure::io(path, &error))?;
        let lines = Lines::new(&text);
        let failures: Vec<Failure> = diagnostics
            .iter()
            .filter_map(|diagnostic| self.signed(path, &lines, diagnostic, only, sink).err())
            .collect();
        sink.faults.extend(failures.into_iter().map(Self::fault));
        Ok(())
    }

    fn report(
        &self,
        root: &Path,
        diagnostics: &[RuffDiagnostic],
        asked: &[&Code],
        only: Option<&RuleId>,
        sink: &mut Sink,
    ) {
        let mut files: BTreeMap<PathBuf, Vec<&RuffDiagnostic>> = BTreeMap::new();
        let failures: Vec<Failure> = diagnostics
            .iter()
            .filter_map(|diagnostic| {
                let absolute = PathBuf::from(&diagnostic.filename);
                let path = match absolute.strip_prefix(root) {
                    Ok(path) => path.to_owned(),
                    Err(_) => return Some(Failure::Outside { path: absolute }),
                };
                if asked.iter().any(|code| code.as_str() == diagnostic.code) {
                    files.entry(path).or_default().push(diagnostic);
                    None
                } else {
                    Some(Failure::Unasked {
                        path,
                        line: diagnostic.location.row,
                        column: diagnostic.location.column,
                        code: diagnostic.code.clone(),
                        message: diagnostic.message.clone(),
                    })
                }
            })
            .collect();
        let read: Vec<Failure> = files
            .iter()
            .filter_map(|(path, diagnostics)| self.file(root, path, diagnostics, only, sink).err())
            .collect();
        sink.faults.extend(failures.into_iter().chain(read).map(Self::fault));
    }

    fn trial(&self, rule: &RuleId, path: &Path, text: &str, sink: &mut Sink) -> Result<(), Failure> {
        let Some(code) = self.route.code(rule) else {
            return Ok(());
        };
        let temp = tempfile::tempdir().map_err(|error| Failure::io(&env::temp_dir(), &error))?;
        let root = fs::canonicalize(temp.path()).map_err(|error| Failure::io(temp.path(), &error))?;
        CONFIGS
            .iter()
            .map(|name| (self.project.join(name), root.join(name)))
            .filter(|(from, _)| from.is_file())
            .try_for_each(|(from, to)| {
                fs::copy(&from, &to)
                    .map(|_| ())
                    .map_err(|error| Failure::io(&from, &error))
            })?;
        let full = root.join(path);
        full.parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| fs::write(&full, text))
            .map_err(|error| Failure::io(path, &error))?;
        let diagnostics = self.diagnostics(&root, &[code], &[path.to_owned()])?;
        self.report(&root, &diagnostics, &[code], Some(rule), sink);
        Ok(())
    }

    /// The engine of the selected ruff rules of what was loaded; none when no ruff rule is selected.
    ///
    /// # Errors
    /// When ruff is nowhere it is looked for, or does not tell its version.
    pub fn new(loaded: &Loaded, selection: &Selection) -> Result<Option<Self>, Fault> {
        let route = RuffRoute::new(
            loaded.catalogues.iter().flat_map(|catalogue| &catalogue.rules),
            |rule| selection.admits(rule),
        );
        let rules: Vec<RuleId> = route.rules().iter().map(|rule| rule.id.clone()).collect();
        if rules.is_empty() {
            return Ok(None);
        }
        let root = &loaded.project.root;
        let fault = |failure| Fault {
            tool: Tool::Ruff,
            failure,
        };
        let path = Tool::Ruff
            .locate(root)
            .ok_or_else(|| fault(Failure::Missing { rules: rules.clone() }))?;
        let version = Tool::Ruff.version(&path, root).map_err(fault)?;
        Ok(Some(Self {
            path,
            version,
            project: root.clone(),
            route,
            rules,
        }))
    }
}

impl BatchEngine for RuffEngine {
    fn name(&self) -> &str {
        Tool::Ruff.name()
    }

    fn version(&self) -> &str {
        &self.version
    }

    fn rules(&self) -> &[RuleId] {
        &self.rules
    }

    fn check(&self, root: &Path, paths: &[PathBuf], sink: &mut Sink) {
        let codes = self.route.codes();
        match self.diagnostics(root, &codes, paths) {
            Ok(diagnostics) => self.report(root, &diagnostics, &codes, None, sink),
            Err(failure) => sink.faults.push(Self::fault(failure)),
        }
    }
}

impl CaseEngine for RuffEngine {
    fn case(&self, rule: &RuleId, path: &Path, text: &str, sink: &mut Sink) {
        if let Err(failure) = self.trial(rule, path, text, sink) {
            sink.faults.push(Self::fault(failure));
        }
    }
}
