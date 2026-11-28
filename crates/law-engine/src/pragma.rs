use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::{Path, PathBuf};

use law_model::{Catalogue, Check, RuffSpec, Span, TySpec};

use crate::directive::Directive;
use crate::engine::{FileEngine, Sink};
use crate::ignore::Ignore;
use crate::noqa::Noqa;
use crate::source::{Source, Syntax};

const COMMENT: &str = "comment";
const IGNORE: &str = "ast-grep-ignore";

/// An inline pragma: where it sits and the rule ids it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pragma {
    /// The file, from the project root.
    pub path: PathBuf,
    /// The comment.
    pub span: Span,
    /// The comment's text.
    pub text: String,
    /// The ids it names: as written in an `ast-grep-ignore`, the catalogue's rules for the codes of a `noqa` or the
    /// names of a `ty: ignore`, every tach rule for a `tach-ignore`; empty when it names none and so silences
    /// everything, as a bare `noqa` or `ty: ignore`.
    pub ids: BTreeSet<String>,
    /// Whether one licensed id licenses it whole: a `tach-ignore`, which names modules, never a rule.
    pub any: bool,
}

impl Pragma {
    /// Where an id is written among the ids of an `ast-grep-ignore`, as bytes of the file: the whole id, never part of
    /// a longer one.
    #[must_use]
    pub fn locate(&self, id: &str) -> Option<Range<usize>> {
        let from = self
            .text
            .find(IGNORE)
            .and_then(|at| self.text[at..].find(':').map(|colon| at + colon + 1))?;
        let boundary = |at: Option<char>| at.is_none_or(|c| c.is_whitespace() || c == ',');
        self.text[from..]
            .match_indices(id)
            .map(|(at, _)| from + at)
            .find(|&start| {
                boundary(self.text[..start].chars().next_back())
                    && boundary(self.text[start + id.len()..].chars().next())
            })
            .map(|start| {
                let at = self.span.bytes.start + start;
                at..at + id.len()
            })
    }
}

/// The engine that finds every inline pragma, in every file, for the exceptions registry to license or report:
/// `ast-grep-ignore` always, ruff's `noqa` when a ruff rule is loaded, ty's `ignore` when a ty rule is, tach's
/// `tach-ignore` when a tach rule is.
#[derive(Debug, Clone)]
pub struct PragmaEngine {
    codes: BTreeMap<String, BTreeSet<String>>,
    ruff: bool,
    ty: Vec<(Option<String>, String)>,
    tach: BTreeSet<String>,
}

impl PragmaEngine {
    fn silences(text: &str) -> bool {
        text.find(char::is_alphabetic)
            .is_some_and(|start| text[start..].starts_with(IGNORE))
    }

    fn ids(text: &str) -> BTreeSet<String> {
        text.trim()
            .split_once(IGNORE)
            .and_then(|(_, after)| after.trim().split_once(':'))
            .map(|(_, ids)| {
                ids.split(',')
                    .filter_map(|id| id.trim_start().split(' ').next())
                    .filter(|id| !id.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn grep(text: &str) -> Option<BTreeSet<String>> {
        Self::silences(text).then(|| Self::ids(text))
    }

    fn noqa(&self, noqa: Noqa) -> Option<BTreeSet<String>> {
        match noqa {
            Noqa::All => self.ruff.then(BTreeSet::new),
            Noqa::Codes(codes) => {
                let ids: BTreeSet<String> = codes
                    .iter()
                    .filter_map(|code| self.codes.get(code))
                    .flatten()
                    .cloned()
                    .collect();
                (!ids.is_empty()).then_some(ids)
            }
        }
    }

    fn named(&self, name: &str) -> BTreeSet<String> {
        let own: BTreeSet<String> = self
            .ty
            .iter()
            .filter(|(rule, _)| rule.as_deref() == Some(name))
            .map(|(_, id)| id.clone())
            .collect();
        if own.is_empty() {
            self.ty
                .iter()
                .filter(|(rule, _)| rule.is_none())
                .map(|(_, id)| id.clone())
                .collect()
        } else {
            own
        }
    }

    fn ignore(&self, ignore: Ignore) -> Option<BTreeSet<String>> {
        match ignore {
            Ignore::All => (!self.ty.is_empty()).then(BTreeSet::new),
            Ignore::Names(names) => {
                let ids: BTreeSet<String> = names.iter().flat_map(|name| self.named(name)).collect();
                (!ids.is_empty()).then_some(ids)
            }
        }
    }

    fn pragmas(&self, text: &str) -> Vec<(BTreeSet<String>, bool)> {
        let grep = Self::grep(text);
        let noqa = Noqa::parse(text).into_iter().filter_map(|noqa| self.noqa(noqa));
        let ignore = Ignore::parse(text).into_iter().filter_map(|ignore| self.ignore(ignore));
        let named = grep.into_iter().chain(noqa).chain(ignore).map(|ids| (ids, false));
        let tach = Directive::parse(text)
            .filter(|_| !self.tach.is_empty())
            .map(|_| (self.tach.clone(), true));
        named.chain(tach).collect()
    }

    /// The engine over the loaded catalogues: which rules each ruff code and each ty name belongs to, and the tach
    /// rules.
    #[must_use]
    pub fn new(catalogues: &[Catalogue]) -> Self {
        let rules = || catalogues.iter().flat_map(|catalogue| &catalogue.rules);
        let codes = rules().fold(BTreeMap::<String, BTreeSet<String>>::new(), |mut codes, rule| {
            if let Check::Ruff(RuffSpec { code, .. }) = &rule.check {
                codes
                    .entry(code.as_str().to_owned())
                    .or_default()
                    .insert(rule.id.as_str().to_owned());
            }
            codes
        });
        let ty = rules()
            .filter_map(|rule| match &rule.check {
                Check::Ty(TySpec { rule: name }) => Some((name.clone(), rule.id.as_str().to_owned())),
                _ => None,
            })
            .collect();
        let tach = rules()
            .filter(|rule| matches!(rule.check, Check::Tach(_)))
            .map(|rule| rule.id.as_str().to_owned())
            .collect();
        Self {
            ruff: !codes.is_empty(),
            codes,
            ty,
            tach,
        }
    }
}

impl FileEngine for PragmaEngine {
    fn admits(&self, _: &Path) -> bool {
        true
    }

    fn check(&self, source: &Source<'_>, sink: &mut Sink) {
        let root = source.root.root();
        let pragmas = root
            .dfs()
            .filter(|node: &Syntax<'_>| node.kind().contains(COMMENT))
            .flat_map(|node| {
                let span = Source::span(&node);
                let text = node.text().into_owned();
                self.pragmas(&text).into_iter().map(move |(ids, any)| Pragma {
                    path: source.path.to_owned(),
                    span: span.clone(),
                    text: text.clone(),
                    ids,
                    any,
                })
            });
        sink.pragmas.extend(pragmas);
    }
}
