const TYPE: &str = "type";
const TY: &str = "ty";
const IGNORE: &str = "ignore";
const PREFIX: &str = "ty:";

/// What a `ty: ignore` or `type: ignore` comment silences in ty, read by ty's grammar
/// (`crates/ty_python_semantic/src/suppression/parser.rs`, ty 0.0.112).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ignore {
    /// Every ty rule: `# ty: ignore`, `# type: ignore`.
    All,
    /// The ty rules named: `# ty: ignore[a, b]`, `# type: ignore[ty:a]`.
    Names(Vec<String>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Type,
    Ty,
}

impl Ignore {
    fn kind(text: &str) -> Option<(Kind, &str)> {
        let (kind, rest) = text
            .strip_prefix(TYPE)
            .map(|rest| (Kind::Type, rest))
            .or_else(|| text.strip_prefix(TY).map(|rest| (Kind::Ty, rest)))?;
        let rest = rest.trim_start().strip_prefix(':')?.trim_start().strip_prefix(IGNORE)?;
        Some((kind, rest))
    }

    fn word(text: &str) -> Option<(&str, &str)> {
        text.starts_with(char::is_alphabetic).then(|| {
            let end = text
                .find(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '-' | ':')))
                .unwrap_or(text.len());
            text.split_at(end)
        })
    }

    fn names(text: &str) -> Option<(Vec<String>, &str)> {
        let mut names = Vec::new();
        let mut rest = text;
        loop {
            rest = rest.trim_start();
            if let Some(after) = rest.strip_prefix(']') {
                return Some((names, after));
            }
            let (name, after) = Self::word(rest)?;
            names.push(name.to_owned());
            rest = after.trim_start();
            match rest.strip_prefix(',') {
                Some(after) => rest = after,
                None => return rest.strip_prefix(']').map(|after| (names, after)),
            }
        }
    }

    fn comment(text: &str) -> Option<(Kind, Option<Vec<String>>)> {
        let (kind, rest) = Self::kind(text.trim_start())?;
        let spaced = rest.starts_with(char::is_whitespace);
        let rest = rest.trim_start();
        let (names, rest) = match rest.strip_prefix('[') {
            Some(inner) => Self::names(inner).map(|(names, rest)| (Some(names), rest))?,
            None => (None, rest),
        };
        (rest.is_empty() || names.is_some() || spaced).then_some((kind, names))
    }

    fn silenced(kind: Kind, names: Option<Vec<String>>) -> Option<Self> {
        let own = |name: String| match kind {
            Kind::Ty => Some(name),
            Kind::Type => name.strip_prefix(PREFIX).map(str::to_owned),
        };
        match names {
            None => Some(Self::All),
            Some(names) => {
                let names: Vec<String> = names.into_iter().filter_map(own).collect();
                (!names.is_empty()).then_some(Self::Names(names))
            }
        }
    }

    /// The suppressions of one comment, one per `#` that opens one; a malformed one, which ty does not honour, an
    /// empty `[]`, and codes that are not ty's are none.
    #[must_use]
    pub fn parse(comment: &str) -> Vec<Self> {
        comment
            .match_indices('#')
            .filter_map(|(at, _)| Self::comment(&comment[at + 1..]))
            .filter_map(|(kind, names)| Self::silenced(kind, names))
            .collect()
    }
}
