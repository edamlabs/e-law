use std::ops::Range;
use std::path::{Path, PathBuf};

use ast_grep_language::{LanguageExt, SupportLang};
use law_model::{Edit, Finding, Fix, Profile, RuleId, Safety};

use crate::ancestry::Ancestry;
use crate::digraph::Graph;
use crate::source::Syntax;

const IMPORT: &str = "import_statement";
const FROM: &str = "import_from_statement";
const ALIASED: &str = "aliased_import";
const RELATIVE: &str = "relative_import";
const ATTRIBUTE: &str = "attribute";
const IDENTIFIER: &str = "identifier";
const STRING: &str = "string";
const TYPE: &str = "type";
const NAME: &str = "name";
const ALIAS: &str = "alias";
const MODULE: &str = "module_name";
const OBJECT: &str = "object";
const DOT: char = '.';
const QUOTES: [char; 2] = ['"', '\''];
const TRIPLE: [&str; 2] = ["\"\"\"", "'''"];
const PREFIXES: &str = "rRuU";
const CRLF: &str = "\r\n";
const LF: &str = "\n";
const JOINT: &str = "; ";
const PYTHON: &str = "py";
const PACKAGE: &str = "__init__";

/// Why a module cannot be moved with every import of it rewritten exactly: the house writes no relative import, and
/// the engine does not guess what one meant.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum Refusal {
    /// A relative import of the module.
    #[error("{}:{line}: a relative import of the module cannot be rewritten", path.display())]
    Relative { path: PathBuf, line: usize },
    /// A relative import inside the module, which means something else one folder deeper.
    #[error("{}:{line}: a relative import in the module changes meaning where it moves", path.display())]
    Within { path: PathBuf, line: usize },
    /// A module of the graph that could not be read.
    #[error("{}: unreadable", path.display())]
    Unread { path: PathBuf },
    /// A module no source root names, or whose new place no root names.
    #[error("{}: no dotted name under a source root, before or after the move", path.display())]
    Unnamed { path: PathBuf },
}

/// What rewrites the project for a module moved: the import graph, the resolver that built it, and what reads a file.
pub(crate) struct Relocation<'a> {
    pub(crate) graph: &'a Graph,
    pub(crate) ancestry: &'a Ancestry,
    pub(crate) read: &'a dyn Fn(&Path) -> Option<String>,
}

/// One module moving.
struct Moving<'a> {
    by: &'a Relocation<'a>,
    from: &'a Path,
    to: &'a Path,
}

/// What is found writing about the module in one file: edits, and the dotted names a plain `import` binds there,
/// which attribute chains and string annotations spell the module by.
#[derive(Default)]
struct Found {
    edits: Vec<(Range<usize>, String)>,
    chains: Vec<(String, String)>,
}

/// An item of a `from` import that names the module: the item, the dotted name it gives, the name it binds.
type Named<'a> = (Syntax<'a>, String, String);

/// A rule whose fix moves a module over its measure into a package, and the profiles whose homes it judges.
pub(crate) struct Relocating {
    pub(crate) rule: RuleId,
    pub(crate) safety: Safety,
    pub(crate) profiles: Vec<Profile>,
}

/// The rules that move modules.
pub(crate) struct Relocator {
    rules: Vec<Relocating>,
}

/// A finding whose fix was not made: which, where, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Refused {
    pub(crate) rule: RuleId,
    pub(crate) safety: Safety,
    pub(crate) path: PathBuf,
    pub(crate) why: Refusal,
}

impl Relocating {
    /// Where a module in a home goes: `<dir>/<name>/<kind>.py`; none when it is already a `<kind>.py` or a package's
    /// `__init__`.
    fn target(&self, path: &Path) -> Option<PathBuf> {
        let kind = self
            .profiles
            .iter()
            .find(|profile| profile.home.iter().any(|glob| glob.matches(path)))?
            .kind
            .as_str();
        let stem = path.file_stem()?.to_str()?;
        (stem != kind && stem != PACKAGE).then(|| path.with_extension("").join(kind).with_extension(PYTHON))
    }
}

impl Relocator {
    /// The relocator over the rules that move.
    pub(crate) fn new(rules: Vec<Relocating>) -> Self {
        Self { rules }
    }

    /// Whether no rule moves.
    pub(crate) fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// The fix of every finding of a rule that moves, where its module can be moved; those that cannot, refused.
    pub(crate) fn attach(&self, relocation: &Relocation<'_>, findings: &mut [Finding]) -> Vec<Refused> {
        findings
            .iter_mut()
            .filter_map(|finding| {
                let rule = self.rules.iter().find(|rule| rule.rule == finding.rule)?;
                let to = rule.target(&finding.path)?;
                match relocation.edits(&finding.path, &to) {
                    Ok(edits) => {
                        finding.fix = Some(Fix {
                            safety: rule.safety,
                            edits,
                        });
                        None
                    }
                    Err(why) => Some(Refused {
                        rule: rule.rule.clone(),
                        safety: rule.safety,
                        path: finding.path.clone(),
                        why,
                    }),
                }
            })
            .collect()
    }
}

impl Relocation<'_> {
    /// The edits that move the module at `from` to `to` and rewrite, exactly, every import of it in the project: the
    /// replacements, file by file, then the move.
    pub(crate) fn edits(&self, from: &Path, to: &Path) -> Result<Vec<Edit>, Refusal> {
        let module = self
            .graph
            .at(from)
            .ok_or_else(|| Refusal::Unnamed { path: from.to_owned() })?;
        let moving = Moving { by: self, from, to };
        moving.within()?;
        let nodes = self.graph.nodes();
        let mut edits: Vec<Edit> = Vec::new();
        for importer in self.graph.importers(module) {
            edits.extend(moving.importer(&nodes[importer].path, nodes[importer].within())?);
        }
        edits.push(Edit::Move {
            from: from.to_owned(),
            to: to.to_owned(),
        });
        Ok(edits)
    }
}

impl Moving<'_> {
    fn text(&self, path: &Path) -> Result<String, Refusal> {
        (self.by.read)(path).ok_or_else(|| Refusal::Unread { path: path.to_owned() })
    }

    fn line(text: &str, at: usize) -> usize {
        text[..at].matches(LF).count() + 1
    }

    fn reaches(&self, module: &str) -> bool {
        self.by
            .ancestry
            .project(module, |path| self.by.graph.at(path).is_some())
            .is_some_and(|path| path == self.from)
    }

    fn rename(&self, written: &str) -> Result<String, Refusal> {
        self.by
            .ancestry
            .respelled(written, self.from, self.to)
            .ok_or_else(|| Refusal::Unnamed {
                path: self.from.to_owned(),
            })
    }

    fn origin(module: &Syntax<'_>) -> String {
        module.text().split_whitespace().collect()
    }

    fn items<'a>(node: &Syntax<'a>) -> Vec<(Syntax<'a>, Syntax<'a>, String)> {
        node.field_children(NAME)
            .filter_map(|item| {
                let (name, binding) = match &*item.kind() {
                    ALIASED => (item.field(NAME)?, item.field(ALIAS)?.text().into_owned()),
                    _ => (item.clone(), item.text().into_owned()),
                };
                Some((item.clone(), name, binding))
            })
            .collect()
    }

    /// The module's own relative imports, which would mean something else one folder deeper.
    fn within(&self) -> Result<(), Refusal> {
        let text = self.text(self.from)?;
        let tree = SupportLang::Python.ast_grep(&text);
        let relative = tree
            .root()
            .dfs()
            .filter(|node| node.kind() == FROM)
            .find(|node| node.field(MODULE).is_some_and(|module| module.kind() == RELATIVE));
        relative.map_or(Ok(()), |node| {
            Err(Refusal::Within {
                path: self.from.to_owned(),
                line: Self::line(&text, node.range().start),
            })
        })
    }

    fn plain(&self, node: &Syntax<'_>, found: &mut Found) -> Result<(), Refusal> {
        for (item, name, _) in Self::items(node) {
            let written: String = name.text().split_whitespace().collect();
            if self.reaches(&written) {
                let renamed = self.rename(&written)?;
                found.edits.push((name.range(), renamed.clone()));
                if item.kind() != ALIASED {
                    found.chains.push((written, renamed));
                }
            }
        }
        Ok(())
    }

    fn separator(text: &str, at: usize) -> String {
        let start = text[..at].rfind(LF).map_or(0, |line| line + 1);
        let prefix = &text[start..at];
        let eol = if text.contains(CRLF) { CRLF } else { LF };
        if prefix.trim().is_empty() {
            format!("{eol}{prefix}")
        } else {
            JOINT.to_owned()
        }
    }

    /// A `from` import that takes the module as a name: it takes it from the new package instead, `from a.b.m import
    /// kind as m`; the statement alone is replaced, one among others splits in two, each name moved leaving it and
    /// the new statement following it.
    fn split(
        &self,
        path: &Path,
        text: &str,
        node: &Syntax<'_>,
        moved: &[Named<'_>],
        found: &mut Found,
    ) -> Result<(), Refusal> {
        let news = moved
            .iter()
            .map(|(_, full, binding)| {
                let renamed = self.rename(full)?;
                let (parent, last) = renamed
                    .rsplit_once(DOT)
                    .ok_or_else(|| Refusal::Unnamed { path: path.to_owned() })?;
                Ok(format!("from {parent} import {last} as {binding}"))
            })
            .collect::<Result<Vec<String>, Refusal>>()?;
        let range = node.range();
        let separator = Self::separator(text, range.start);
        let ranges: Vec<Range<usize>> = Self::items(node).iter().map(|(item, ..)| item.range()).collect();
        if moved.len() == ranges.len() {
            found.edits.push((range, news.join(&separator)));
            return Ok(());
        }
        let gone = |at: usize| moved.iter().any(|(item, ..)| item.range() == ranges[at]);
        let last = (0..ranges.len()).rev().find(|&at| !gone(at)).unwrap_or_default();
        for at in (0..last).filter(|&at| gone(at)) {
            found
                .edits
                .push((ranges[at].start..ranges[at + 1].start, String::new()));
        }
        if let Some(end) = ranges.last().filter(|_| last + 1 < ranges.len()) {
            found.edits.push((ranges[last].end..end.end, String::new()));
        }
        found
            .edits
            .push((range.end..range.end, format!("{separator}{}", news.join(&separator))));
        Ok(())
    }

    fn from(&self, path: &Path, text: &str, within: &str, node: &Syntax<'_>, found: &mut Found) -> Result<(), Refusal> {
        let Some(module) = node.field(MODULE) else {
            return Ok(());
        };
        let Some(base) = Ancestry::anchored(within, &Self::origin(&module)) else {
            return Ok(());
        };
        let moved: Vec<Named<'_>> = Self::items(node)
            .into_iter()
            .map(|(item, name, binding)| {
                let name = name.text().into_owned();
                let full = if base.is_empty() {
                    name
                } else {
                    format!("{base}{DOT}{name}")
                };
                (item, full, binding)
            })
            .filter(|(_, full, _)| self.reaches(full))
            .collect();
        let whole = self.reaches(&base);
        if module.kind() == RELATIVE && (whole || !moved.is_empty()) {
            return Err(Refusal::Relative {
                path: path.to_owned(),
                line: Self::line(text, node.range().start),
            });
        }
        if whole {
            found.edits.push((module.range(), self.rename(&base)?));
            Ok(())
        } else if moved.is_empty() {
            Ok(())
        } else {
            self.split(path, text, node, &moved, found)
        }
    }

    fn spelled(node: &Syntax<'_>) -> Option<String> {
        match &*node.kind() {
            IDENTIFIER => Some(node.text().into_owned()),
            ATTRIBUTE => {
                let object = Self::spelled(&node.field(OBJECT)?)?;
                Some(format!("{object}{DOT}{}", node.field(ATTRIBUTE)?.text()))
            }
            _ => None,
        }
    }

    fn chained(node: &Syntax<'_>, chains: &[(String, String)], found: &mut Found) {
        let whole = Self::spelled(node);
        let object = node.field(OBJECT).filter(|object| object.kind() == IDENTIFIER);
        found.edits.extend(chains.iter().filter_map(|(written, renamed)| {
            let bytes = match (&whole, &object) {
                (Some(name), _) if name == written => Some(node.range()),
                (_, Some(object)) if !written.contains(DOT) && object.text() == written.as_str() => {
                    Some(object.range())
                }
                _ => None,
            };
            bytes.map(|bytes| (bytes, renamed.clone()))
        }));
    }

    /// A string annotation, its dotted names that begin with a name a plain `import` bound rewritten.
    fn quoted(node: &Syntax<'_>, chains: &[(String, String)], found: &mut Found) {
        let text = node.text();
        let Some(open) = text.find(QUOTES) else {
            return;
        };
        if !text[..open].chars().all(|letter| PREFIXES.contains(letter)) {
            return;
        }
        let width = if TRIPLE.iter().any(|triple| text[open..].starts_with(triple)) {
            3
        } else {
            1
        };
        let start = open + width;
        let Some(inside) = text.get(start..text.len().saturating_sub(width).max(start)) else {
            return;
        };
        let base = node.range().start + start;
        let mut at = 0;
        while at < inside.len() {
            let rest = &inside[at..];
            let run = rest
                .find(|letter: char| !(letter.is_alphanumeric() || letter == '_' || letter == DOT))
                .unwrap_or(rest.len());
            let token = &rest[..run];
            let spelled = chains.iter().find(|(written, _)| {
                token
                    .strip_prefix(written.as_str())
                    .is_some_and(|tail| tail.is_empty() || tail.starts_with(DOT))
            });
            if let Some((written, renamed)) = spelled {
                found
                    .edits
                    .push((base + at..base + at + written.len(), renamed.clone()));
            }
            at += if run == 0 {
                rest.chars().next().map_or(1, char::len_utf8)
            } else {
                run
            };
        }
    }

    /// The edits in one importer: its import statements, then the attribute chains and annotation strings that spell
    /// by a name a plain `import` of the module bound.
    fn importer(&self, path: &Path, within: &str) -> Result<Vec<Edit>, Refusal> {
        let text = self.text(path)?;
        let tree = SupportLang::Python.ast_grep(&text);
        let root = tree.root();
        let mut found = Found::default();
        for node in root.dfs() {
            match &*node.kind() {
                IMPORT => self.plain(&node, &mut found)?,
                FROM => self.from(path, &text, within, &node, &mut found)?,
                _ => {}
            }
        }
        let mut chains = std::mem::take(&mut found.chains);
        chains.sort();
        chains.dedup();
        if !chains.is_empty() {
            for node in root.dfs() {
                match &*node.kind() {
                    ATTRIBUTE => Self::chained(&node, &chains, &mut found),
                    STRING if node.ancestors().any(|above| above.kind() == TYPE) => {
                        Self::quoted(&node, &chains, &mut found);
                    }
                    _ => {}
                }
            }
        }
        found.edits.sort_by_key(|(bytes, _)| (bytes.start, bytes.end));
        Ok(found
            .edits
            .into_iter()
            .map(|(bytes, text)| Edit::Replace {
                path: path.to_owned(),
                bytes,
                text,
            })
            .collect())
    }
}
