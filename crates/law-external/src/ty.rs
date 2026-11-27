use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};

use law_config::{Loaded, Selection};
use law_engine::{BatchEngine, Sink};
use law_model::{Position, RuleId};

use crate::fault::{Failure, Fault};
use crate::lines::Lines;
use crate::route::TyRoute;
use crate::sarif::{Sarif, SarifResult};
use crate::tool::Tool;

type Placed<'a> = (&'a SarifResult, Position, Position);

const SYNTAX: &str = "invalid-syntax";
const ARGS: [&str; 6] = [
    "check",
    "--output-format",
    "sarif",
    "--exit-zero",
    "--no-progress",
    "--",
];

/// The ty engine: one `ty check` per run in the project's own environment, its SARIF results signed by the rules
/// they route to, a file it cannot parse a fault; ty offers no fix.
#[derive(Debug, Clone)]
pub struct TyEngine {
    path: PathBuf,
    version: String,
    route: TyRoute,
    rules: Vec<RuleId>,
}

impl TyEngine {
    fn fault(failure: Failure) -> law_engine::Fault {
        Fault {
            tool: Tool::Ty,
            failure,
        }
        .into()
    }

    fn sarif(&self, root: &Path, paths: &[PathBuf]) -> Result<Sarif, Failure> {
        let args: Vec<OsString> = ARGS
            .iter()
            .map(OsString::from)
            .chain(paths.iter().map(OsString::from))
            .collect();
        let args: Vec<&OsStr> = args.iter().map(OsString::as_os_str).collect();
        let out = Tool::Ty.run(&self.path, root, &args)?;
        serde_json::from_slice(&out).map_err(|error| Failure::Document {
            message: error.to_string(),
        })
    }

    fn placed(root: &Path, result: &SarifResult) -> Result<(PathBuf, Position, Position), Failure> {
        let physical = &result
            .locations
            .first()
            .ok_or_else(|| Failure::Placeless {
                rule: result.rule_id.clone(),
                message: result.message.text.clone(),
            })?
            .physical_location;
        let uri = &physical.artifact_location.uri;
        let absolute = physical.artifact_location.path().ok_or_else(|| Failure::Document {
            message: format!("`{uri}` is not a file URI"),
        })?;
        let path = absolute
            .strip_prefix(root)
            .map_err(|_| Failure::Outside { path: absolute.clone() })?
            .to_owned();
        let region = physical.region;
        let start = Position {
            line: region.start_line,
            column: region.start_column,
        };
        let end = Position {
            line: region.end_line,
            column: region.end_column,
        };
        Ok((path, start, end))
    }

    fn file(&self, root: &Path, path: &Path, results: &[Placed<'_>], sink: &mut Sink) -> Vec<Failure> {
        let text = match fs::read_to_string(root.join(path)) {
            Ok(text) => text,
            Err(error) => return vec![Failure::io(path, &error)],
        };
        let lines = Lines::new(&text);
        results
            .iter()
            .filter_map(|(result, start, end)| {
                if result.rule_id == SYNTAX {
                    return Some(Failure::Unasked {
                        path: path.to_owned(),
                        line: start.line,
                        column: start.column,
                        code: result.rule_id.clone(),
                        message: result.message.text.clone(),
                    });
                }
                let Some(span) = lines.span(*start, *end) else {
                    return Some(Failure::Place {
                        path: path.to_owned(),
                        line: start.line,
                        column: start.column,
                    });
                };
                self.route
                    .route(&result.rule_id)
                    .into_iter()
                    .for_each(|rule| sink.report(rule, path, span.clone(), Some(&result.message.text), None));
                None
            })
            .collect()
    }

    fn report(&self, root: &Path, sarif: &Sarif, sink: &mut Sink) {
        let mut files: BTreeMap<PathBuf, Vec<Placed<'_>>> = BTreeMap::new();
        let unplaced: Vec<Failure> = sarif
            .runs
            .iter()
            .flat_map(|run| &run.results)
            .filter_map(|result| match Self::placed(root, result) {
                Ok((path, start, end)) => {
                    files.entry(path).or_default().push((result, start, end));
                    None
                }
                Err(failure) => Some(failure),
            })
            .collect();
        let unread: Vec<Failure> = files
            .iter()
            .flat_map(|(path, results)| self.file(root, path, results, sink))
            .collect();
        sink.faults.extend(unplaced.into_iter().chain(unread).map(Self::fault));
    }

    /// The engine of the selected ty rules of what was loaded; none when no ty rule is selected.
    ///
    /// # Errors
    /// When ty is nowhere it is looked for, or does not tell its version.
    pub fn new(loaded: &Loaded, selection: &Selection) -> Result<Option<Self>, Fault> {
        let route = TyRoute::new(
            loaded.catalogues.iter().flat_map(|catalogue| &catalogue.rules),
            |rule| selection.admits(rule),
        );
        let rules: Vec<RuleId> = route.rules().iter().map(|rule| rule.id.clone()).collect();
        if rules.is_empty() {
            return Ok(None);
        }
        let root = &loaded.project.root;
        let fault = |failure| Fault {
            tool: Tool::Ty,
            failure,
        };
        let path = Tool::Ty
            .locate(root)
            .ok_or_else(|| fault(Failure::Missing { rules: rules.clone() }))?;
        let version = Tool::Ty.version(&path, root).map_err(fault)?;
        Ok(Some(Self {
            path,
            version,
            route,
            rules,
        }))
    }
}

impl BatchEngine for TyEngine {
    fn name(&self) -> &str {
        Tool::Ty.name()
    }

    fn version(&self) -> &str {
        &self.version
    }

    fn rules(&self) -> &[RuleId] {
        &self.rules
    }

    fn check(&self, root: &Path, paths: &[PathBuf], sink: &mut Sink) {
        match self.sarif(root, paths) {
            Ok(sarif) => self.report(root, &sarif, sink),
            Err(failure) => sink.faults.push(Self::fault(failure)),
        }
    }
}
