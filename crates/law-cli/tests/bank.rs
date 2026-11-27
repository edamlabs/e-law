mod common;

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use common::{Outcome, Project};
use serde_json::Value;

const BANK: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../law-engine/src/builtins.json");
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
const GENERATOR: &str = "scripts/builtins.py";
const HEADS: [&str; 6] = ["aliases", "classes", "factories", "platform", "python", "wrappers"];
const OBJECT: &str = "builtins.object";

type Fault = Box<dyn std::error::Error>;

fn bank() -> Result<Value, Fault> {
    Ok(serde_json::from_str(&fs::read_to_string(BANK)?)?)
}

fn classes(bank: &Value) -> Result<&serde_json::Map<String, Value>, Fault> {
    Ok(bank["classes"].as_object().ok_or("no classes")?)
}

fn texts(value: &Value) -> Result<Vec<&str>, Fault> {
    value
        .as_array()
        .ok_or("not a list")?
        .iter()
        .map(|item| item.as_str().ok_or_else(|| "not a text".into()))
        .collect()
}

fn interpreter(version: &str) -> Option<PathBuf> {
    let out = Command::new("uv").args(["python", "find", version]).output().ok()?;
    let python = PathBuf::from(String::from_utf8(out.stdout).ok()?.trim());
    python.is_file().then_some(python)
}

#[test]
fn bank_embedded_file_has_the_shape_the_engine_decodes() -> Outcome {
    let bank = bank()?;
    let heads: BTreeSet<&str> = bank
        .as_object()
        .ok_or("no object")?
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(heads, BTreeSet::from(HEADS));
    assert_eq!(bank["platform"], "linux");
    assert!(
        bank["python"]
            .as_str()
            .is_some_and(|version| version.starts_with("3.14.")),
        "{}",
        bank["python"]
    );
    let classes = classes(&bank)?;
    assert!(classes.len() > 500, "{} classes", classes.len());
    for (key, entry) in classes {
        let fields: BTreeSet<&str> = entry
            .as_object()
            .ok_or("no entry")?
            .keys()
            .map(String::as_str)
            .collect();
        assert!(
            fields.is_superset(&BTreeSet::from(["bases", "methods"]))
                && fields.is_subset(&BTreeSet::from(["abstract", "bases", "methods"])),
            "{key}: {fields:?}"
        );
        assert!(key.contains('.'), "{key} is not module.Name");
        if let Some(abstracts) = entry.get("abstract") {
            let names = texts(abstracts)?;
            let methods = texts(&entry["methods"])?;
            assert!(!names.is_empty(), "{key}: empty abstract");
            assert!(names.is_sorted(), "{key}: abstract unsorted");
            assert!(
                names.iter().all(|name| methods.contains(name)),
                "{key}: abstract outside methods"
            );
        }
    }
    Ok(())
}

#[test]
fn bank_written_modules_carry_their_classes_and_the_abstract_names() -> Outcome {
    let bank = bank()?;
    let classes = classes(&bank)?;
    for key in [
        "abc.ABC",
        "collections.abc.Iterator",
        "contextlib.AbstractContextManager",
        "dataclasses.Field",
        "enum.StrEnum",
        "typing.Protocol",
    ] {
        assert!(classes.contains_key(key), "{key}");
    }
    assert_eq!(
        texts(&classes["contextlib.AbstractContextManager"]["abstract"])?,
        ["__exit__"]
    );
    assert_eq!(classes["abc.ABC"].get("abstract"), None);
    assert_eq!(classes["enum.StrEnum"].get("abstract"), None);
    Ok(())
}

#[test]
fn bank_keys_and_methods_are_sorted_so_the_file_is_deterministic() -> Outcome {
    let bank = bank()?;
    let classes = classes(&bank)?;
    let keys: Vec<&String> = classes.keys().collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted);
    for (key, entry) in classes {
        let methods = texts(&entry["methods"])?;
        assert!(methods.is_sorted(), "{key}: methods are not sorted");
    }
    Ok(())
}

#[test]
fn bank_holds_the_builtins_the_engine_resolves_a_bare_base_through() -> Outcome {
    let bank = bank()?;
    let classes = classes(&bank)?;
    for (key, public, base) in [
        ("builtins.dict", "update", OBJECT),
        ("builtins.list", "append", OBJECT),
        ("builtins.str", "upper", OBJECT),
        ("builtins.int", "bit_length", OBJECT),
        ("builtins.tuple", "count", OBJECT),
        ("builtins.BaseException", "add_note", OBJECT),
        ("builtins.Exception", "__init__", "builtins.BaseException"),
    ] {
        let entry = classes.get(key).ok_or(key)?;
        assert!(texts(&entry["bases"])?.contains(&base), "{key}: {entry}");
        assert!(
            key == "builtins.Exception" || texts(&entry["methods"])?.contains(&public),
            "{key} lacks {public}"
        );
    }
    assert_eq!(classes[OBJECT]["bases"], serde_json::json!([]));
    assert_eq!(bank["aliases"]["collections.abc"], "_collections_abc");
    Ok(())
}

#[test]
fn bank_declares_the_functions_that_become_classes() -> Outcome {
    let bank = bank()?;
    assert_eq!(
        bank["factories"],
        serde_json::json!({
            "typing.NamedTuple": ["builtins.tuple"],
            "typing.TypedDict": [],
            "typing_extensions.NamedTuple": ["builtins.tuple"],
            "typing_extensions.TypedDict": []
        })
    );
    assert!(texts(&bank["wrappers"])?.contains(&"typing._alias"));
    Ok(())
}

#[test]
fn bank_every_base_is_a_class_of_the_bank() -> Outcome {
    let bank = bank()?;
    let classes = classes(&bank)?;
    let dangling: BTreeSet<String> = classes
        .values()
        .map(|entry| texts(&entry["bases"]))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .filter(|base| !classes.contains_key(*base))
        .map(str::to_owned)
        .collect();
    assert!(dangling.is_empty(), "bases the bank does not hold: {dangling:?}");
    Ok(())
}

#[test]
fn bank_embedded_decodes_and_is_not_empty_through_a_check() -> Outcome {
    let project = Project::with_rules(
        "[\"src\"]",
        "- id: t.members\n  message: genealogy\n  scope: {include: [\"src/**/*.py\"]}\n  check: {engine: members, lineage: {common: [common]}}\n  fix: {safety: none, reason: x}\n",
    )?;
    project.file(
        "src/pkg/m.py",
        "class Bag(dict):\n    def _update_x(self): ...\n\n\nclass Many(list):\n    def _append_x(self): ...\n",
    )?;
    let out = project.check(&["-v"])?;
    assert_eq!(
        (out.stdout.as_str(), out.code),
        ("All checks passed.\n", 0),
        "{}",
        out.stderr
    );
    assert!(!out.stderr.contains("does not decode"), "{}", out.stderr);
    assert!(
        out.stderr
            .contains("2 classes judged, 0 skipped (unresolved base), 2 on a builtin base"),
        "{}",
        out.stderr
    );
    Ok(())
}

#[test]
fn bank_regenerates_byte_identical_from_its_generator() -> Outcome {
    let kept = fs::read_to_string(BANK)?;
    let version = serde_json::from_str::<Value>(&kept)?["python"]
        .as_str()
        .ok_or("no python")?
        .to_owned();
    // The bank names the interpreter it came from; regeneration is deliberate, never "any 3.14".
    let Some(python) = interpreter(&version) else {
        writeln!(io::stderr(), "SKIP: uv-managed python {version} cannot be found")?;
        return Ok(());
    };
    let out = Command::new("uv")
        .args(["run", "--no-project", "--python"])
        .arg(&python)
        .args(["python", GENERATOR])
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .current_dir(Path::new(ROOT))
        .output()?;
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let fresh = String::from_utf8(out.stdout)?;
    assert_eq!(
        serde_json::from_str::<Value>(&fresh)?["python"],
        serde_json::from_str::<Value>(&kept)?["python"],
        "the bank was generated from another interpreter"
    );
    assert!(fresh == kept, "the bank differs from what scripts/builtins.py writes");
    Ok(())
}
