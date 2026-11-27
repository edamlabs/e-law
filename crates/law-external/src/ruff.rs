use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;

use law_config::{Loaded, Selection};
use law_engine::{BatchEngine, CaseEngine, Sink};
use law_model::{Check, Edit, Fix, Rule, RuleId};

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
const CONFIG: &str = "--config";
const END: &str = "--";
const COMMA: &str = ",";
const CONFIGS: [&str; 3] = [".ruff.toml", "ruff.toml", "pyproject.toml"];
const PANICKED: &str = "reading a ruff command panicked";

/// The selected rules that run ruff with the same settings: the `--config` arguments they make, and where their
/// diagnostics go.
#[derive(Debug, Clone)]
struct Group {
    settings: Vec<String>,
    route: RuffRoute,
}

/// The ruff engine: one `ruff check` per distinct settings set among the selected rules — rules without settings share
/// one — over the codes those rules name, each diagnostic signed by the rules of its set it routes to, ruff's fix
/// carried at the weaker of ruff's and the rule's safety.
#[derive(Debug, Clone)]
pub struct RuffEngine {
    path: PathBuf,
    version: String,
    project: PathBuf,
    groups: Vec<Group>,
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

    fn diagnostics(&self, cwd: &Path, group: &Group, paths: &[PathBuf]) -> Result<Vec<RuffDiagnostic>, Failure> {
        let select = group
            .route
            .codes()
            .iter()
            .map(|code| code.as_str())
            .collect::<Vec<_>>()
            .join(COMMA);
        let args: Vec<OsString> = ARGS
            .iter()
            .map(OsString::from)
            .chain([OsString::from(select)])
            .chain(
                group
                    .settings
                    .iter()
                    .flat_map(|setting| [OsString::from(CONFIG), OsString::from(setting)]),
            )
            .chain([OsString::from(END)])
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
        route: &RuffRoute,
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
        route
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
        route: &RuffRoute,
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
            .filter_map(|diagnostic| Self::signed(route, path, &lines, diagnostic, only, sink).err())
            .collect();
        sink.faults.extend(failures.into_iter().map(Self::fault));
        Ok(())
    }

    fn report(route: &RuffRoute, root: &Path, diagnostics: &[RuffDiagnostic], only: Option<&RuleId>, sink: &mut Sink) {
        let asked = route.codes();
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
            .filter_map(|(path, diagnostics)| Self::file(route, root, path, diagnostics, only, sink).err())
            .collect();
        sink.faults.extend(failures.into_iter().chain(read).map(Self::fault));
    }

    fn trial(&self, rule: &RuleId, path: &Path, text: &str, sink: &mut Sink) -> Result<(), Failure> {
        let Some(group) = self.groups.iter().find(|group| group.route.code(rule).is_some()) else {
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
        let diagnostics = self.diagnostics(&root, group, &[path.to_owned()])?;
        Self::report(&group.route, &root, &diagnostics, Some(rule), sink);
        Ok(())
    }

    fn overrides(rule: &Rule) -> Option<Vec<String>> {
        match &rule.check {
            Check::Ruff(spec) => Some(spec.overrides()),
            _ => None,
        }
    }

    fn groups(loaded: &Loaded, selection: &Selection) -> Vec<Group> {
        let rules = || loaded.catalogues.iter().flat_map(|catalogue| &catalogue.rules);
        let sets: BTreeSet<Vec<String>> = rules()
            .filter(|rule| selection.admits(rule))
            .filter_map(Self::overrides)
            .collect();
        sets.into_iter()
            .map(|settings| Group {
                route: RuffRoute::new(
                    rules().filter(|rule| Self::overrides(rule).as_ref() == Some(&settings)),
                    |rule| selection.admits(rule),
                ),
                settings,
            })
            .collect()
    }

    /// The engine of the selected ruff rules of what was loaded; none when no ruff rule is selected.
    ///
    /// # Errors
    /// When ruff is nowhere it is looked for, or does not tell its version.
    pub fn new(loaded: &Loaded, selection: &Selection) -> Result<Option<Self>, Fault> {
        let groups = Self::groups(loaded, selection);
        let rules: Vec<RuleId> = groups
            .iter()
            .flat_map(|group| group.route.rules())
            .map(|rule| rule.id.clone())
            .collect();
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
            groups,
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
        let results: Vec<Result<Vec<RuffDiagnostic>, Failure>> = thread::scope(|scope| {
            self.groups
                .iter()
                .map(|group| scope.spawn(move || self.diagnostics(root, group, paths)))
                .collect::<Vec<_>>()
                .into_iter()
                .map(|running| {
                    running.join().unwrap_or_else(|_| {
                        Err(Failure::Refused {
                            message: PANICKED.to_owned(),
                        })
                    })
                })
                .collect()
        });
        self.groups
            .iter()
            .zip(results)
            .for_each(|(group, result)| match result {
                Ok(diagnostics) => Self::report(&group.route, root, &diagnostics, None, sink),
                Err(failure) => sink.faults.push(Self::fault(failure)),
            });
    }
}

impl CaseEngine for RuffEngine {
    fn case(&self, rule: &RuleId, path: &Path, text: &str, sink: &mut Sink) {
        if let Err(failure) = self.trial(rule, path, text, sink) {
            sink.faults.push(Self::fault(failure));
        }
    }
}
