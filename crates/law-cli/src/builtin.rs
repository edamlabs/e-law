use std::path::PathBuf;

use law_config::{Bundle, Origin};

const EAGER: &str = "eager";
const FILES: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/eager.rs"));

/// The catalogues the tool carries inside itself.
pub struct Builtin;

impl Builtin {
    /// Where a built-in catalogue is read from, by its name: `eager`, from the files embedded at build; none for any
    /// other name.
    pub fn resolve(name: &str) -> Option<Origin> {
        (name == EAGER).then(|| {
            Origin::Bundle(Bundle {
                name: EAGER.to_owned(),
                files: FILES
                    .iter()
                    .map(|(path, text)| (PathBuf::from(path), (*text).to_owned()))
                    .collect(),
            })
        })
    }
}
