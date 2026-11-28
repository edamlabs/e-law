const IGNORE: &str = "tach-ignore";

/// A `tach-ignore` directive, read by tach's grammar (`# *tach-ignore(?:\(([^)]*)\))?((?:\s+[\w.]+)*)\s*$`, searched
/// in the comment, tach 0.35.3): its reason, and the modules it names; none named, it silences every import of its
/// line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directive {
    /// The reason between parentheses, as written; none without them.
    pub reason: Option<String>,
    /// The modules named, in order.
    pub modules: Vec<String>,
}

impl Directive {
    fn word(token: &str) -> bool {
        token.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '.'))
    }

    fn after(text: &str) -> Option<Self> {
        let rest = text.trim_start_matches(' ').strip_prefix(IGNORE)?;
        let (reason, rest) = match rest.strip_prefix('(') {
            Some(inner) => inner
                .split_once(')')
                .map(|(reason, rest)| (Some(reason.to_owned()), rest))?,
            None => (None, rest),
        };
        let spaced = rest.trim().is_empty() || rest.starts_with(char::is_whitespace);
        let modules: Vec<String> = rest.split_whitespace().map(str::to_owned).collect();
        (spaced && modules.iter().all(|module| Self::word(module))).then_some(Self { reason, modules })
    }

    /// The directive of a comment, from the first `#` that opens one; none when the comment holds none tach honours.
    #[must_use]
    pub fn parse(comment: &str) -> Option<Self> {
        comment
            .match_indices('#')
            .find_map(|(at, _)| Self::after(&comment[at + 1..]))
    }
}
