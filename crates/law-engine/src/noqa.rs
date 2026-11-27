const NOQA: &str = "noqa";
const FILE: [&str; 2] = ["ruff", "flake8"];

/// What a `noqa` directive silences, read by ruff's grammar (`crates/ruff_linter/src/noqa.rs`, ruff 0.17.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Noqa {
    /// Every code: `# noqa`, `# ruff: noqa`.
    All,
    /// The codes named: `# noqa: A1, B2`, `# ruff: noqa: A1`.
    Codes(Vec<String>),
}

impl Noqa {
    fn noqa(text: &str) -> Option<&str> {
        text.get(..NOQA.len())
            .filter(|head| head.eq_ignore_ascii_case(NOQA))
            .map(|_| &text[NOQA.len()..])
    }

    fn hashes(comment: &str) -> impl Iterator<Item = &str> {
        comment.match_indices('#').map(|(at, _)| comment[at + 1..].trim_start())
    }

    fn inline(comment: &str) -> Option<Self> {
        Self::hashes(comment).find_map(Self::noqa).and_then(Self::directive)
    }

    fn file(comment: &str) -> Option<Self> {
        Self::hashes(comment)
            .find_map(|text| {
                FILE.iter()
                    .find_map(|tool| text.strip_prefix(tool))
                    .and_then(|rest| rest.trim_start().strip_prefix(':'))
                    .and_then(|rest| Self::noqa(rest.trim_start()))
            })
            .and_then(Self::directive)
    }

    fn directive(after: &str) -> Option<Self> {
        match after.chars().next() {
            None | Some('#') => Some(Self::All),
            Some(':') => Self::codes(&after[1..]),
            Some(c) if c.is_whitespace() => after
                .trim_start()
                .strip_prefix(':')
                .map_or(Some(Self::All), Self::codes),
            Some(_) => None,
        }
    }

    fn code(rest: &str) -> Option<(&str, &str)> {
        let upper = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_uppercase()).len();
        let digits = rest[upper..].len() - rest[upper..].trim_start_matches(|c: char| c.is_ascii_digit()).len();
        (upper > 0 && digits > 0).then(|| rest.split_at(upper + digits))
    }

    fn codes(text: &str) -> Option<Self> {
        let mut codes = Vec::new();
        let mut rest = text;
        let mut squashed = false;
        loop {
            rest = rest.trim_start();
            if let Some(after) = rest.strip_prefix(',') {
                rest = after;
                continue;
            }
            let starts_upper = rest.starts_with(|c: char| c.is_ascii_uppercase());
            match Self::code(rest) {
                Some((code, after)) => {
                    codes.push(code.to_owned());
                    rest = after;
                    match rest.chars().next() {
                        None => break,
                        Some(',') => {
                            rest = &rest[1..];
                            squashed = false;
                        }
                        Some(c) if c.is_whitespace() || c == '#' => squashed = false,
                        Some(c) if c.is_ascii_uppercase() => squashed = true,
                        Some(_) => return None,
                    }
                }
                None if starts_upper && squashed => return None,
                None => break,
            }
        }
        (!codes.is_empty()).then_some(Self::Codes(codes))
    }

    /// The directives of one comment: its inline `noqa`, then its file-level one; a malformed one, which ruff
    /// ignores, is none.
    #[must_use]
    pub fn parse(comment: &str) -> Vec<Self> {
        [Self::inline(comment), Self::file(comment)]
            .into_iter()
            .flatten()
            .collect()
    }
}
