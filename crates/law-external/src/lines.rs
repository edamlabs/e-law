use std::iter;

use law_model::{Position, Span};

/// Where every line of a text starts, in bytes: a line ends after `\n`, `\r\n` or a lone `\r`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lines<'a> {
    text: &'a str,
    starts: Vec<usize>,
}

impl<'a> Lines<'a> {
    /// The lines of a text.
    #[must_use]
    pub fn new(text: &'a str) -> Self {
        let bytes = text.as_bytes();
        let breaks = bytes.iter().enumerate().filter_map(|(at, byte)| match byte {
            b'\n' => Some(at + 1),
            b'\r' if bytes.get(at + 1) != Some(&b'\n') => Some(at + 1),
            _ => None,
        });
        Self {
            text,
            starts: iter::once(0).chain(breaks).collect(),
        }
    }

    /// The byte offset of a 1-based line and a 1-based column in characters; none when the text has no such place.
    #[must_use]
    pub fn offset(&self, line: u32, column: u32) -> Option<usize> {
        let index = usize::try_from(line).ok()?.checked_sub(1)?;
        let skip = usize::try_from(column).ok()?.checked_sub(1)?;
        let start = *self.starts.get(index)?;
        let end = self.starts.get(index + 1).copied().unwrap_or(self.text.len());
        self.text[start..end]
            .char_indices()
            .map(|(at, _)| start + at)
            .chain(iter::once(end))
            .nth(skip)
    }

    /// The span of a 1-based line, its break left out; none when the text has no such line.
    #[must_use]
    pub fn line(&self, line: u32) -> Option<Span> {
        let index = usize::try_from(line).ok()?.checked_sub(1)?;
        let start = *self.starts.get(index)?;
        let end = self.starts.get(index + 1).copied().unwrap_or(self.text.len());
        let text = self.text[start..end].trim_end_matches(['\n', '\r']);
        let column = u32::try_from(text.chars().count()).ok()?.checked_add(1)?;
        Some(Span {
            start: Position { line, column: 1 },
            end: Position { line, column },
            bytes: start..start + text.len(),
        })
    }

    /// The span between two places, each a 1-based line and a 1-based column in characters; none when one is missing.
    #[must_use]
    pub fn span(&self, start: Position, end: Position) -> Option<Span> {
        let from = self.offset(start.line, start.column)?;
        let to = self.offset(end.line, end.column)?;
        (from <= to).then_some(Span {
            start,
            end,
            bytes: from..to,
        })
    }
}
