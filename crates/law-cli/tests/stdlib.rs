mod common;

use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;

use common::{Outcome, Project};

const LINEAGE: &str = "lineage: {common: [common]}";
const MARKER: &str = " t.members genealogy: ";
const SOURCE: &str = "src/pkg/m.py";

type Fault = Box<dyn std::error::Error>;

/// What a run said: its findings as `line detail`, and its stderr census lines.
struct Seen {
    found: Vec<String>,
    notes: Vec<String>,
}

fn rule() -> String {
    format!(
        "- id: t.members\n  message: genealogy\n  scope: {{include: [\"src/**/*.py\"]}}\n  check: {{engine: members, {LINEAGE}}}\n  fix: {{safety: none, reason: x}}\n"
    )
}

/// The uv-managed Python 3.14 whose standard library is the real one; none when uv or the interpreter is absent.
fn interpreter() -> Option<PathBuf> {
    let out = Command::new("uv").args(["python", "find", "3.14"]).output().ok()?;
    let python = PathBuf::from(String::from_utf8(out.stdout).ok()?.trim());
    let bin = python.parent()?.to_path_buf();
    bin.parent()?.join("lib/python3.14/typing.py").is_file().then_some(bin)
}

fn seen(project: &Project, code: &str) -> Result<Seen, Fault> {
    project.file(SOURCE, code)?;
    let out = project.check(&["-v"])?;
    let found = out
        .stdout
        .lines()
        .filter_map(|line| {
            let (place, detail) = line.split_once(MARKER)?;
            let mut parts = place.trim_end_matches(':').rsplitn(3, ':');
            let (_, number) = (parts.next()?, parts.next()?);
            Some(format!("{number} {detail}"))
        })
        .collect();
    let notes = out
        .stderr
        .lines()
        .filter(|line| line.starts_with("members:"))
        .map(str::to_owned)
        .collect();
    Ok(Seen { found, notes })
}

fn bare(code: &str) -> Result<Seen, Fault> {
    seen(&Project::with_rules("[\"src\"]", &rule())?, code)
}

fn real(code: &str, site: &[(&str, &str)]) -> Result<Option<Seen>, Fault> {
    let Some(bin) = interpreter() else {
        writeln!(io::stderr(), "SKIP: uv-managed python 3.14 cannot be found")?;
        return Ok(None);
    };
    let project = Project::with_rules("[\"src\"]", &rule())?;
    project.file(
        ".venv/pyvenv.cfg",
        &format!("home = {}\nversion_info = 3.14.0.final.0\n", bin.display()),
    )?;
    project.dir(".venv/lib/python3.14/site-packages")?;
    site.iter().try_for_each(|(path, text)| {
        project
            .file(&format!(".venv/lib/python3.14/site-packages/{path}"), text)
            .map(drop)
    })?;
    Ok(Some(seen(&project, code)?))
}

fn census(seen: &Seen) -> &str {
    seen.notes.get(1).map_or("", String::as_str)
}

fn judged(seen: &Seen, classes: usize) -> bool {
    census(seen).starts_with(&format!("members: {classes} classes judged, 0 skipped"))
}

#[test]
fn stdlib_builtin_bases_serve_their_own_publics_and_a_stray_is_an_orphan() -> Outcome {
    for (base, served) in [
        ("dict", "_update_x"),
        ("list", "_append_x"),
        ("set", "_add_x"),
        ("tuple", "_count_x"),
        ("str", "_upper_x"),
        ("bytes", "_decode_x"),
        ("int", "_bit_length_x"),
        ("float", "_hex_x"),
        ("frozenset", "_union_x"),
        ("bytearray", "_extend_x"),
        ("Exception", "_add_note_x"),
        ("BaseException", "_with_traceback_x"),
        ("OSError", "_add_note_x"),
    ] {
        let code = format!("class A({base}):\n    def {served}(self): ...\n    def _stray(self): ...\n");
        let seen = bare(&code)?;
        assert_eq!(
            seen.found,
            ["3 _stray serves no public of A"],
            "{base}: {:?}",
            seen.notes
        );
        assert!(judged(&seen, 1), "{base}: {:?}", seen.notes);
    }
    Ok(())
}

#[test]
fn stdlib_enum_bases_serve_their_inherited_builtin_publics() -> Outcome {
    let Some(seen) = real(
        "from enum import Enum, IntEnum, StrEnum\n\n\nclass S(StrEnum):\n    ONE = \"one\"\n\n    def _upper_x(self): ...\n    def _stray(self): ...\n\n\nclass I(IntEnum):\n    ONE = 1\n\n    def _bit_length_x(self): ...\n    def _stray(self): ...\n\n\nclass E(Enum):\n    ONE = 1\n\n    def _name_x(self): ...\n    def _stray(self): ...\n",
        &[],
    )?
    else {
        return Ok(());
    };
    assert_eq!(
        seen.found,
        [
            "8 _stray serves no public of S",
            "15 _stray serves no public of I",
            "22 _stray serves no public of E"
        ]
    );
    assert!(judged(&seen, 3), "{:?}", seen.notes);
    Ok(())
}

#[test]
fn stdlib_a_bare_name_the_bank_does_not_hold_is_unresolved_and_the_class_is_not_judged() -> Outcome {
    let seen = bare("class A(Frobnicate):\n    def _stray(self): ...\n")?;
    assert!(seen.found.is_empty(), "{:?}", seen.found);
    assert_eq!(
        seen.notes[1..],
        [
            "members: 0 classes judged, 1 skipped (unresolved base), 0 on a builtin base; modules read: 0 project, 0 foreign",
            "members: unresolved: Frobnicate \u{d7}1 (no such name)"
        ]
    );
    Ok(())
}

#[test]
fn stdlib_object_contributes_no_publics_to_the_bank_line() -> Outcome {
    let seen = bare("class A(object):\n    def _stray(self): ...\n    def _hash_x(self): ...\n")?;
    assert_eq!(
        seen.found,
        ["2 _stray serves no public of A", "3 _hash_x serves no public of A"]
    );
    Ok(())
}

#[test]
fn stdlib_collections_abc_resolves_to_the_file_the_stdlib_aliases_it_to() -> Outcome {
    let Some(seen) = real(
        "import collections.abc\nfrom collections.abc import Mapping\n\n\nclass A(Mapping):\n    def _items_x(self): ...\n    def _stray(self): ...\n\n\nclass B(collections.abc.MutableMapping):\n    def _update_x(self): ...\n    def _stray(self): ...\n",
        &[],
    )?
    else {
        return Ok(());
    };
    assert_eq!(
        seen.found,
        ["7 _stray serves no public of A", "12 _stray serves no public of B"]
    );
    assert!(judged(&seen, 2), "{:?}", seen.notes);
    assert!(census(&seen).ends_with("1 foreign"), "{:?}", seen.notes);
    Ok(())
}

#[test]
fn stdlib_typing_alias_follows_the_first_argument_to_its_target() -> Outcome {
    let Some(seen) = real(
        "import typing\nfrom typing import Mapping, Sequence\n\n\nclass A(Mapping):\n    def _items_x(self): ...\n    def _stray(self): ...\n\n\nclass B(typing.Sequence):\n    def _index_x(self): ...\n    def _stray(self): ...\n\n\nclass C(Sequence):\n    def _count_x(self): ...\n    def _stray(self): ...\n",
        &[],
    )?
    else {
        return Ok(());
    };
    assert_eq!(
        seen.found,
        [
            "7 _stray serves no public of A",
            "12 _stray serves no public of B",
            "17 _stray serves no public of C"
        ]
    );
    assert!(judged(&seen, 3), "{:?}", seen.notes);
    Ok(())
}

#[test]
fn stdlib_typeddict_is_judged_with_no_inherited_publics() -> Outcome {
    let Some(seen) = real(
        "from typing import TypedDict\n\n\nclass T(TypedDict):\n    size: int\n\n    def _get_x(self): ...\n    def _stray(self): ...\n",
        &[],
    )?
    else {
        return Ok(());
    };
    assert_eq!(
        seen.found,
        ["7 _get_x serves no public of T", "8 _stray serves no public of T"]
    );
    assert!(judged(&seen, 1), "{:?}", seen.notes);
    Ok(())
}

#[test]
fn stdlib_namedtuple_is_judged_with_tuples_publics() -> Outcome {
    let Some(seen) = real(
        "from typing import NamedTuple\n\n\nclass P(NamedTuple):\n    size: int\n\n    def _count_x(self): ...\n    def _index_x(self): ...\n    def _stray(self): ...\n",
        &[],
    )?
    else {
        return Ok(());
    };
    assert_eq!(seen.found, ["9 _stray serves no public of P"]);
    assert!(judged(&seen, 1), "{:?}", seen.notes);
    Ok(())
}

#[test]
fn stdlib_typing_extensions_factories_are_judged_like_the_typing_ones() -> Outcome {
    let site = [(
        "typing_extensions.py",
        "import typing\n\n\n@_special\ndef TypedDict(typename, fields=None, /, *, total=True): ...\n\n\n@_special\ndef NamedTuple(typename, fields=None, /): ...\n",
    )];
    let Some(seen) = real(
        "from typing_extensions import NamedTuple, TypedDict\n\n\nclass T(TypedDict):\n    size: int\n\n    def _get_x(self): ...\n\n\nclass P(NamedTuple):\n    size: int\n\n    def _count_x(self): ...\n    def _stray(self): ...\n",
        &site,
    )?
    else {
        return Ok(());
    };
    assert_eq!(
        seen.found,
        ["7 _get_x serves no public of T", "14 _stray serves no public of P"]
    );
    assert!(judged(&seen, 2), "{:?}", seen.notes);
    Ok(())
}

#[test]
fn stdlib_a_function_outside_the_banks_factories_is_not_a_base() -> Outcome {
    let site = [("lib1/__init__.py", "def Thing(name): ...\n")];
    let Some(seen) = real(
        "from lib1 import Thing\n\n\nclass A(Thing):\n    def _stray(self): ...\n",
        &site,
    )?
    else {
        return Ok(());
    };
    assert!(seen.found.is_empty(), "{:?}", seen.found);
    assert!(census(&seen).contains("1 skipped"), "{:?}", seen.notes);
    Ok(())
}

#[test]
fn stdlib_the_real_typing_module_is_on_disk_where_the_fixture_points() -> Outcome {
    let Some(bin) = interpreter() else {
        writeln!(io::stderr(), "SKIP: uv-managed python 3.14 cannot be found")?;
        return Ok(());
    };
    let typing = fs::read_to_string(bin.join("../lib/python3.14/typing.py"))?;
    assert!(typing.contains("Mapping = _alias(collections.abc.Mapping, 2)"));
    assert!(typing.contains("def TypedDict("));
    assert!(typing.contains("def NamedTuple("));
    Ok(())
}
