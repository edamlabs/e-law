use std::path::Path;

use ast_grep_core::tree_sitter::StrDoc;
use ast_grep_core::{AstGrep, Node};
use ast_grep_language::SupportLang;
use law_model::{Position, Span};

/// A Python file parsed once.
pub type Tree = AstGrep<StrDoc<SupportLang>>;

/// A node of a parsed Python file.
pub type Syntax<'a> = Node<'a, StrDoc<SupportLang>>;

/// What a file engine is handed: the path from the root, the text, and its one parse.
pub struct Source<'a> {
    /// The path, from the project root.
    pub path: &'a Path,
    /// The text.
    pub text: &'a str,
    /// The parse.
    pub root: &'a Tree,
}

impl Source<'_> {
    fn position(node: &Syntax<'_>, at: &ast_grep_core::Position) -> Position {
        Position {
            line: u32::try_from(at.line() + 1).unwrap_or(u32::MAX),
            column: u32::try_from(at.column(node) + 1).unwrap_or(u32::MAX),
        }
    }

    /// Where a node sits: 1-based lines, 1-based columns in characters, and its bytes.
    #[must_use]
    pub fn span(node: &Syntax<'_>) -> Span {
        Span {
            start: Self::position(node, &node.start_pos()),
            end: Self::position(node, &node.end_pos()),
            bytes: node.range(),
        }
    }
}
