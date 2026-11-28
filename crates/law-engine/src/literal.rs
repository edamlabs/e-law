use std::ops::Range;
use std::path::{Path, PathBuf};

use ast_grep_language::{LanguageExt, SupportLang};

use crate::source::Syntax;

const STRING: &str = "string";
const START: &str = "string_start";
const CONTENT: &str = "string_content";
const END: &str = "string_end";
const STATEMENT: &str = "expression_statement";
const QUOTES: [char; 2] = ['"', '\''];
const PREFIXES: &str = "rRuU";
const DOT: char = '.';

/// A string literal that is nothing but a dotted name of identifiers, the way `mocker.patch("pkg.mod.X")` writes a
/// module: where its text sits, and what it says. Byte strings, f-strings, strings with an escape, docstrings and
/// bare string statements are not names, whatever they spell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Literal {
    pub(crate) path: PathBuf,
    pub(crate) bytes: Range<usize>,
    pub(crate) value: String,
}

impl Literal {
    fn identifier(segment: &str) -> bool {
        let mut letters = segment.chars();
        letters
            .next()
            .is_some_and(|first| first.is_alphabetic() || first == '_')
            && letters.all(|letter| letter.is_alphanumeric() || letter == '_')
    }

    fn dotted(value: &str) -> bool {
        value.contains(DOT) && value.split(DOT).all(Self::identifier)
    }

    fn plain(start: &Syntax<'_>) -> bool {
        let text = start.text();
        text.split(QUOTES)
            .next()
            .is_some_and(|prefix| prefix.chars().all(|letter| PREFIXES.contains(letter)))
    }

    fn of(path: &Path, node: &Syntax<'_>) -> Option<Self> {
        if node.parent().is_some_and(|above| above.kind() == STATEMENT) {
            return None;
        }
        let mut content = None;
        for child in node.children() {
            match &*child.kind() {
                START if Self::plain(&child) => {}
                END => {}
                CONTENT if content.is_none() => content = Some(child),
                _ => return None,
            }
        }
        let content = content?;
        let value = content.text().into_owned();
        Self::dotted(&value).then(|| Self {
            path: path.to_owned(),
            bytes: content.range(),
            value,
        })
    }

    /// The dotted-name literals of one Python file.
    pub(crate) fn all(path: &Path, text: &str) -> Vec<Self> {
        let tree = SupportLang::Python.ast_grep(text);
        tree.root()
            .dfs()
            .filter(|node| node.kind() == STRING)
            .filter_map(|node| Self::of(path, &node))
            .collect()
    }
}
