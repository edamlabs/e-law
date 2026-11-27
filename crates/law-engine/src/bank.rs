use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use serde::Deserialize;

use crate::fault::Fault;

const TEXT: &str = include_str!("builtins.json");

static BANK: LazyLock<Result<Bank, Fault>> = LazyLock::new(|| Bank::decode(TEXT));
static EMPTY: LazyLock<Bank> = LazyLock::new(Bank::default);

/// A class of the bank: its bases, by bank key or dotted name, the methods it writes, and those of them it leaves
/// abstract.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Entry {
    pub(crate) bases: Vec<String>,
    pub(crate) methods: Vec<String>,
    #[serde(default, rename = "abstract")]
    pub(crate) pending: BTreeSet<String>,
}

/// The interpreter's own classes, read off a live interpreter by `scripts/builtins.py` and embedded: every class of
/// `builtins`, of the fileless stdlib modules and of the few stdlib modules a lone file bases on, by `module.name` —
/// answering only for a module no root holds; the modules the stdlib aliases to another file;
/// the functions that become classes through `__mro_entries__`, with the bases they stand for; the calls whose first
/// argument a generic alias of `typing` stands for. The file also records the interpreter it was read from.
#[derive(Debug, Default, Deserialize)]
pub struct Bank {
    pub(crate) aliases: BTreeMap<String, String>,
    pub(crate) factories: BTreeMap<String, Vec<String>>,
    pub(crate) wrappers: BTreeSet<String>,
    pub(crate) classes: BTreeMap<String, Entry>,
}

impl Bank {
    fn dangling(&self) -> Option<String> {
        self.classes.iter().find_map(|(key, entry)| {
            entry
                .bases
                .iter()
                .find(|base| !self.classes.contains_key(*base))
                .map(|base| format!("`{key}` names the base `{base}`, which it does not hold"))
        })
    }

    /// A bank decoded from its JSON text, whole: every base a class names is a class of the bank.
    ///
    /// # Errors
    /// `Fault::Bank` when the text is no bank, or a class names a base the bank does not hold.
    pub fn decode(text: &str) -> Result<Self, Fault> {
        let bank: Self = serde_json::from_str(text).map_err(|error| Fault::Bank {
            message: error.to_string(),
        })?;
        bank.dangling().map_or(Ok(bank), |message| Err(Fault::Bank { message }))
    }

    /// The embedded bank, or the fault that kept it from decoding.
    pub(crate) fn embedded() -> Result<&'static Self, &'static Fault> {
        BANK.as_ref()
    }

    /// The fault of an embedded bank that does not decode; none when it does.
    pub(crate) fn fault() -> Option<Fault> {
        Self::embedded().err().cloned()
    }

    /// The embedded bank; empty when it does not decode, which the run reports as a fault.
    pub(crate) fn get() -> &'static Self {
        Self::embedded().unwrap_or(&EMPTY)
    }

    /// The module a dotted module name is read from: the file the stdlib aliases it to, or itself.
    pub(crate) fn module<'a>(&'a self, module: &'a str) -> &'a str {
        self.aliases.get(module).map_or(module, String::as_str)
    }
}
