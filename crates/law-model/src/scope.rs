use std::iter::Peekable;
use std::path::PathBuf;
use std::str::Chars;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::glob::Glob;
use crate::profile::Kind;

const SAMPLE: &str = "case.py";
const STAR: &str = "case";
const ANY: char = 'x';
const POOL: &str = "xabcdefghijklmnopqrstuvwyz0123456789_";

/// Where a rule looks: globs from the project root, and optionally one entity kind.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    /// The paths looked at.
    #[serde(default = "Scope::python")]
    pub include: Vec<Glob>,
    /// The paths skipped among those included.
    #[serde(default)]
    pub exclude: Vec<Glob>,
    /// The entity kind, declared by a profile, the rule is limited to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<Kind>,
}

struct Sample<'a> {
    chars: Peekable<Chars<'a>>,
    out: String,
}

impl Sample<'_> {
    fn of(glob: &str) -> PathBuf {
        let mut sample = Sample {
            chars: glob.chars().peekable(),
            out: String::new(),
        };
        sample.walk(false);
        PathBuf::from(sample.out)
    }

    fn walk(&mut self, nested: bool) {
        while let Some(c) = self.chars.peek().copied() {
            match c {
                ',' | '}' if nested => return,
                _ => {
                    self.chars.next();
                    self.step(c);
                }
            }
        }
    }

    fn step(&mut self, c: char) {
        match c {
            '*' => self.star(),
            '?' => self.out.push(ANY),
            '[' => self.class(),
            '{' => self.alternate(),
            '\\' => self.out.extend(self.chars.next()),
            c => self.out.push(c),
        }
    }

    fn star(&mut self) {
        let double = self.chars.next_if_eq(&'*').is_some();
        let opens = self.out.is_empty() || self.out.ends_with('/');
        match (double && opens, self.chars.peek()) {
            (true, None) => self.out.push_str(SAMPLE),
            (true, Some('/')) => {
                self.chars.next();
            }
            _ => self.out.push_str(STAR),
        }
    }

    fn class(&mut self) {
        let negated = self.chars.next_if(|c| matches!(c, '!' | '^')).is_some();
        let mut members = String::new();
        members.extend(self.chars.next());
        members.extend(self.chars.by_ref().take_while(|c| *c != ']'));
        let ranges: Vec<(char, char)> = Self::ranges(&members);
        let inside = |c: char| ranges.iter().any(|(low, high)| (*low..=*high).contains(&c));
        let chosen = if negated {
            POOL.chars().find(|c| !inside(*c))
        } else {
            ranges.first().map(|(low, _)| *low)
        };
        self.out.extend(chosen);
    }

    fn ranges(members: &str) -> Vec<(char, char)> {
        let chars: Vec<char> = members.chars().collect();
        let mut rest = chars.as_slice();
        let mut ranges = Vec::new();
        loop {
            match rest {
                [low, '-', high, tail @ ..] => {
                    ranges.push((*low, *high));
                    rest = tail;
                }
                [low, tail @ ..] => {
                    ranges.push((*low, *low));
                    rest = tail;
                }
                [] => return ranges,
            }
        }
    }

    fn alternate(&mut self) {
        self.walk(true);
        let mut depth = 0usize;
        while let Some(c) = self.chars.next() {
            match c {
                '{' => depth += 1,
                '}' if depth == 0 => return,
                '}' => depth -= 1,
                '\\' => {
                    self.chars.next();
                }
                _ => {}
            }
        }
    }
}

impl Scope {
    fn python() -> Vec<Glob> {
        "**/*.py".parse().into_iter().collect()
    }

    /// A path synthesised from the first `include` glob, matched by it, where a case with no path of its own is placed.
    #[must_use]
    pub fn sample(&self) -> PathBuf {
        self.include
            .first()
            .map_or_else(|| PathBuf::from(SAMPLE), |glob| Sample::of(glob.as_str()))
    }
}

impl Default for Scope {
    fn default() -> Self {
        Self {
            include: Self::python(),
            exclude: Vec::new(),
            kind: None,
        }
    }
}
