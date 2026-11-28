mod common;

use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;

use common::{Out, Outcome, Project};

type Fault = Box<dyn std::error::Error>;
type Files<'a> = &'a [(&'a str, &'a str)];

const MARKER: &str = " t.p m: ";
const MODULE: &str = "app/pkg/u/m.py";
const HOME: &str = "- kind: unit\n  home: [\"app/pkg/u/*.py\"]\n  suffix: Unit\n";
const METHODS: &str = "  methods: [go]\n";
const TWO_METHODS: &str = "  methods: [go, stop]\n";
const ATTRIBUTES: &str = "  attributes: [spec]\n";
const LINE: &str = "  line: [typing.Protocol]\n";
const BASES: &str = "  bases: [typing.Protocol]\n";
const BASES_BOTH: &str = "  bases: [typing.Protocol, typing_extensions.Protocol]\n";
const NEVER: &str = "  never: [abc.ABC]\n";
const ABSTRACTS: [&str; 6] = [
    "abstractmethod",
    "abc.abstractmethod",
    "abstractproperty",
    "abstractclassmethod",
    "abstractstaticmethod",
    "somewhere.abstractmethod",
];

/// What a run said: its findings as `path:line detail`, its profile census line, the whole output.
struct Seen {
    found: Vec<String>,
    census: Option<String>,
    out: Out,
}

fn profile(extra: &str) -> String {
    format!("{HOME}{extra}")
}

fn rule(kinds: &str, aspect: &str, scope: &str) -> String {
    format!(
        "- id: t.p\n  message: m\n  scope: {scope}\n  check: {{engine: profile, kinds: [{kinds}], aspect: {aspect}}}\n  fix: {{safety: none, reason: x}}\n"
    )
}

fn world(profiles: &str, rules: &str) -> Result<Project, Fault> {
    let project = Project::with_rules("[\"app\"]", rules)?;
    project.file("cat/profiles/p.yml", profiles)?;
    Ok(project)
}

fn read(out: Out) -> Seen {
    let found = out
        .stdout
        .lines()
        .filter_map(|line| {
            let (place, detail) = line.split_once(MARKER)?;
            let mut parts = place.trim_end_matches(':').rsplitn(3, ':');
            let (_, number, path) = (parts.next()?, parts.next()?, parts.next()?);
            Some(format!("{path}:{number} {detail}"))
        })
        .collect();
    let census = out
        .stderr
        .lines()
        .find(|line| line.starts_with("profile:"))
        .map(str::to_owned);
    Seen { found, census, out }
}

fn scene(profiles: &str, aspect: &str, files: Files<'_>) -> Result<Seen, Fault> {
    let project = world(profiles, &rule("unit", aspect, "{}"))?;
    files
        .iter()
        .try_for_each(|(path, text)| project.file(path, text).map(drop))?;
    Ok(read(project.check(&["-v"])?))
}

fn solo(profiles: &str, aspect: &str, code: &str) -> Result<Vec<String>, Fault> {
    Ok(scene(profiles, aspect, &[(MODULE, code)])?.found)
}

fn here(line: u32, detail: &str) -> String {
    format!("{MODULE}:{line} {detail}")
}

/// The uv-managed Python 3.14 whose standard library is the real one; none when uv or the interpreter is absent.
fn interpreter() -> Option<PathBuf> {
    let out = Command::new("uv").args(["python", "find", "3.14"]).output().ok()?;
    let python = PathBuf::from(String::from_utf8(out.stdout).ok()?.trim());
    let bin = python.parent()?.to_path_buf();
    bin.parent()?.join("lib/python3.14/typing.py").is_file().then_some(bin)
}

fn fake_environment(project: &Project, site: Files<'_>) -> Result<(), Fault> {
    let root = project.path().display();
    project.file(
        ".venv/pyvenv.cfg",
        &format!("home = {root}/py/bin\nversion_info = 3.14.0.final.0\n"),
    )?;
    project.dir("py/lib/python3.14")?;
    project.dir(".venv/lib/python3.14/site-packages")?;
    site.iter().try_for_each(|(path, text)| {
        project
            .file(&format!(".venv/lib/python3.14/site-packages/{path}"), text)
            .map(drop)
    })
}

fn foreign(profiles: &str, aspect: &str, code: &str, site: Files<'_>) -> Result<Seen, Fault> {
    let project = world(profiles, &rule("unit", aspect, "{}"))?;
    fake_environment(&project, site)?;
    project.file(MODULE, code)?;
    Ok(read(project.check(&["-v"])?))
}

fn real(profiles: &str, aspect: &str, code: &str) -> Result<Option<Seen>, Fault> {
    let Some(bin) = interpreter() else {
        writeln!(io::stderr(), "SKIP: uv-managed python 3.14 cannot be found")?;
        return Ok(None);
    };
    let project = world(profiles, &rule("unit", aspect, "{}"))?;
    project.file(
        ".venv/pyvenv.cfg",
        &format!("home = {}\nversion_info = 3.14.0.final.0\n", bin.display()),
    )?;
    project.dir(".venv/lib/python3.14/site-packages")?;
    project.file(MODULE, code)?;
    Ok(Some(read(project.check(&["-v"])?)))
}

///// present /////

#[test]
fn profile_present_a_module_holding_an_entity_passes() -> Outcome {
    assert_eq!(
        solo(&profile(""), "present", "class AUnit: ...\n")?,
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn profile_present_a_module_with_no_entity_fails_at_line_one() -> Outcome {
    let seen = scene(&profile(""), "present", &[(MODULE, "x = 1\n\n\nclass Row: ...\n")])?;
    assert_eq!(seen.found, [here(1, "no class ends in Unit")]);
    assert!(
        seen.out.stdout.starts_with(&format!("{MODULE}:1:1: t.p")),
        "{}",
        seen.out.stdout
    );
    assert_eq!(seen.out.code, 1);
    Ok(())
}

#[test]
fn profile_present_among_several_classes_one_entity_is_enough() -> Outcome {
    let code = "class Row: ...\n\n\nclass Helper: ...\n\n\nclass AUnit: ...\n\n\nclass Tail: ...\n";
    assert_eq!(solo(&profile(""), "present", code)?, Vec::<String>::new());
    Ok(())
}

#[test]
fn profile_present_a_nested_class_is_no_entity_and_an_empty_module_has_none() -> Outcome {
    let nested = solo(&profile(""), "present", "class Outer:\n    class InnerUnit: ...\n")?;
    assert_eq!(nested, [here(1, "no class ends in Unit")]);
    let empty = solo(&profile(""), "present", "")?;
    assert_eq!(empty, [here(1, "no class ends in Unit")]);
    Ok(())
}

#[test]
fn profile_present_judges_each_module_of_the_home_alone() -> Outcome {
    let seen = scene(
        &profile(""),
        "present",
        &[
            ("app/pkg/u/a.py", "class AUnit: ...\n"),
            ("app/pkg/u/b.py", "class Row: ...\n"),
            ("app/pkg/elsewhere/c.py", "class Row: ...\n"),
        ],
    )?;
    assert_eq!(seen.found, ["app/pkg/u/b.py:1 no class ends in Unit"]);
    Ok(())
}

///// surname /////

#[test]
fn profile_surname_every_class_ending_in_it_passes() -> Outcome {
    let code = "class AUnit: ...\n\n\nclass BUnit(AUnit): ...\n";
    assert_eq!(solo(&profile(""), "surname", code)?, Vec::<String>::new());
    Ok(())
}

#[test]
fn profile_surname_each_stray_class_of_a_module_with_several_is_found_at_its_name() -> Outcome {
    let code = "class AUnit: ...\n\n\nclass Row: ...\n\n\nclass BUnit(AUnit): ...\n\n\nclass Helper(AUnit): ...\n";
    let seen = scene(&profile(""), "surname", &[(MODULE, code)])?;
    assert_eq!(
        seen.found,
        [
            here(4, "Row does not end in Unit"),
            here(10, "Helper does not end in Unit")
        ]
    );
    assert!(
        seen.out.stdout.starts_with(&format!("{MODULE}:4:7: t.p")),
        "{}",
        seen.out.stdout
    );
    Ok(())
}

#[test]
fn profile_surname_a_nested_class_is_not_judged() -> Outcome {
    let code = "class AUnit:\n    class Row: ...\n";
    assert_eq!(solo(&profile(""), "surname", code)?, Vec::<String>::new());
    Ok(())
}

///// methods /////

#[test]
fn profile_methods_own_method_passes_async_too() -> Outcome {
    let code = "class AUnit:\n    def go(self): ...\n\n\nclass BUnit:\n    async def go(self): ...\n";
    assert_eq!(solo(&profile(METHODS), "methods", code)?, Vec::<String>::new());
    Ok(())
}

#[test]
fn profile_methods_each_missing_method_is_one_finding_at_the_class_name() -> Outcome {
    let seen = scene(
        &profile(TWO_METHODS),
        "methods",
        &[(MODULE, "class AUnit: ...\n\n\nclass BUnit:\n    def stop(self): ...\n")],
    )?;
    assert_eq!(
        seen.found,
        [
            here(1, "AUnit lacks go"),
            here(1, "AUnit lacks stop"),
            here(4, "BUnit lacks go")
        ]
    );
    assert!(
        seen.out.stdout.starts_with(&format!("{MODULE}:1:7: t.p")),
        "{}",
        seen.out.stdout
    );
    Ok(())
}

#[test]
fn profile_methods_inherited_through_a_same_file_base() -> Outcome {
    let code = "class Base:\n    def go(self): ...\n\n\nclass Mid(Base): ...\n\n\nclass AUnit(Mid): ...\n";
    assert_eq!(solo(&profile(METHODS), "methods", code)?, Vec::<String>::new());
    let lacking = "class Base:\n    def other(self): ...\n\n\nclass AUnit(Base): ...\n";
    assert_eq!(
        solo(&profile(METHODS), "methods", lacking)?,
        [here(5, "AUnit lacks go")]
    );
    Ok(())
}

#[test]
fn profile_methods_inherited_from_an_imported_project_base() -> Outcome {
    let seen = scene(
        &profile(METHODS),
        "methods",
        &[
            (
                "app/pkg/base.py",
                "class Base:\n    def go(self): ...\n\n\nclass Dry:\n    def other(self): ...\n",
            ),
            (
                MODULE,
                "from pkg.base import Base, Dry\n\n\nclass AUnit(Base): ...\n\n\nclass BUnit(Dry): ...\n",
            ),
        ],
    )?;
    assert_eq!(seen.found, [here(7, "BUnit lacks go")]);
    assert_eq!(
        seen.census.as_deref(),
        Some("profile: 2 classes judged, 0 skipped (unresolved base)")
    );
    Ok(())
}

#[test]
fn profile_methods_inherited_from_a_foreign_base_of_the_environment() -> Outcome {
    let site: Files<'_> = &[(
        "foreignlib.py",
        "class Base:\n    def go(self): ...\n\n\nclass Dry: ...\n",
    )];
    let code = "from foreignlib import Base, Dry\n\n\nclass AUnit(Base): ...\n\n\nclass BUnit(Dry): ...\n";
    let seen = foreign(&profile(METHODS), "methods", code, site)?;
    assert_eq!(seen.found, [here(7, "BUnit lacks go")]);
    assert_eq!(
        seen.census.as_deref(),
        Some("profile: 2 classes judged, 0 skipped (unresolved base)")
    );
    Ok(())
}

#[test]
fn profile_methods_inherited_from_a_bank_class_with_no_environment() -> Outcome {
    let profiles = profile("  methods: [__enter__, __exit__]\n");
    let code = "from contextlib import AbstractContextManager\n\n\nclass AUnit(AbstractContextManager): ...\n\n\nclass BUnit: ...\n";
    let seen = scene(&profiles, "methods", &[(MODULE, code)])?;
    assert_eq!(
        seen.found,
        [here(7, "BUnit lacks __enter__"), here(7, "BUnit lacks __exit__")]
    );
    assert_eq!(
        seen.census.as_deref(),
        Some("profile: 2 classes judged, 0 skipped (unresolved base)")
    );
    Ok(())
}

#[test]
fn profile_methods_an_unresolved_base_is_not_judged_and_is_counted() -> Outcome {
    let code = "from nowhere import Base\n\n\nclass AUnit(Base): ...\n\n\nclass BUnit: ...\n";
    let seen = scene(&profile(METHODS), "methods", &[(MODULE, code)])?;
    assert_eq!(seen.found, [here(7, "BUnit lacks go")]);
    assert_eq!(
        seen.census.as_deref(),
        Some("profile: 1 classes judged, 1 skipped (unresolved base)")
    );
    Ok(())
}

#[test]
fn profile_methods_real_stdlib_class_resolves_through_the_interpreter() -> Outcome {
    let code = "import queue\n\n\nclass AUnit(queue.Queue): ...\n";
    let Some(seen) = real(&profile("  methods: [get, put]\n"), "methods", code)? else {
        return Ok(());
    };
    assert_eq!(seen.found, Vec::<String>::new());
    assert_eq!(
        seen.census.as_deref(),
        Some("profile: 1 classes judged, 0 skipped (unresolved base)")
    );
    let lacking = "import queue\n\n\nclass AUnit(queue.Queue): ...\n";
    let Some(seen) = real(&profile("  methods: [get, absent]\n"), "methods", lacking)? else {
        return Ok(());
    };
    assert_eq!(seen.found, [here(4, "AUnit lacks absent")]);
    Ok(())
}

///// attributes /////

#[test]
fn profile_attributes_valued_own_binding_passes() -> Outcome {
    let code = "class AUnit:\n    spec = 1\n\n\nclass BUnit:\n    spec: ClassVar[int] = 1\n";
    assert_eq!(solo(&profile(ATTRIBUTES), "attributes", code)?, Vec::<String>::new());
    Ok(())
}

#[test]
fn profile_attributes_a_binding_in_a_body_level_block_or_a_target_tuple_counts() -> Outcome {
    let codes = [
        "class AUnit:\n    if FLAG:\n        spec = 1\n",
        "class AUnit:\n    if FLAG:\n        other = 1\n    else:\n        spec = 1\n",
        "class AUnit:\n    if FLAG:\n        other = 1\n    elif OTHER:\n        spec = 1\n",
        "class AUnit:\n    if TYPE_CHECKING:\n        other = 1\n    else:\n        spec = 1\n",
        "class AUnit:\n    try:\n        spec = 1\n    except ImportError:\n        other = 1\n",
        "class AUnit:\n    try:\n        other = 1\n    except ImportError:\n        spec = 1\n",
        "class AUnit:\n    try:\n        other = 1\n    finally:\n        spec = 1\n",
        "class AUnit:\n    try:\n        other = 1\n    except ImportError:\n        pass\n    else:\n        spec = 1\n",
        "class AUnit:\n    spec, other = 1, 2\n",
        "class AUnit:\n    other, spec = 1, 2\n",
        "class AUnit:\n    [other, spec] = 1, 2\n",
        "class AUnit:\n    (other, (spec, third)) = 1, (2, 3)\n",
    ];
    for code in codes {
        assert_eq!(
            solo(&profile(ATTRIBUTES), "attributes", code)?,
            Vec::<String>::new(),
            "{code}"
        );
    }
    Ok(())
}

#[test]
fn profile_attributes_a_loop_target_an_import_a_def_or_a_class_is_no_binding() -> Outcome {
    let codes = [
        "class AUnit:\n    for spec in range(3):\n        pass\n",
        "class AUnit:\n    import spec\n",
        "class AUnit:\n    from os import spec\n",
        "class AUnit:\n    def spec(self): ...\n",
        "class AUnit:\n    class spec: ...\n",
    ];
    for code in codes {
        assert_eq!(
            solo(&profile(ATTRIBUTES), "attributes", code)?,
            [here(1, "AUnit lacks spec")],
            "{code}"
        );
    }
    Ok(())
}

#[test]
fn profile_attributes_missing_binding_is_found_at_the_class_name() -> Outcome {
    let seen = scene(
        &profile(ATTRIBUTES),
        "attributes",
        &[(MODULE, "class AUnit:\n    other = 1\n")],
    )?;
    assert_eq!(seen.found, [here(1, "AUnit lacks spec")]);
    assert!(
        seen.out.stdout.starts_with(&format!("{MODULE}:1:7: t.p")),
        "{}",
        seen.out.stdout
    );
    Ok(())
}

#[test]
fn profile_attributes_valued_binding_inherited_from_a_same_file_base() -> Outcome {
    let code = "class Base:\n    spec = 1\n\n\nclass AUnit(Base): ...\n";
    assert_eq!(solo(&profile(ATTRIBUTES), "attributes", code)?, Vec::<String>::new());
    Ok(())
}

#[test]
fn profile_attributes_valued_binding_inherited_from_an_imported_project_base() -> Outcome {
    let seen = scene(
        &profile(ATTRIBUTES),
        "attributes",
        &[
            ("app/pkg/base.py", "class Base:\n    spec = 1\n"),
            (MODULE, "from pkg.base import Base\n\n\nclass AUnit(Base): ...\n"),
        ],
    )?;
    assert_eq!(seen.found, Vec::<String>::new());
    Ok(())
}

#[test]
fn profile_attributes_valued_binding_inherited_from_a_foreign_base() -> Outcome {
    let site: Files<'_> = &[(
        "foreignlib.py",
        "class Base:\n    spec = 1\n\n\nclass Dry:\n    spec: int\n",
    )];
    let code = "from foreignlib import Base, Dry\n\n\nclass AUnit(Base): ...\n\n\nclass BUnit(Dry): ...\n";
    let seen = foreign(&profile(ATTRIBUTES), "attributes", code, site)?;
    assert_eq!(seen.found, [here(7, "BUnit lacks spec")]);
    Ok(())
}

#[test]
fn profile_attributes_an_annotation_alone_in_a_concrete_class_is_not_declared() -> Outcome {
    let code = "class AUnit:\n    spec: int\n";
    assert_eq!(
        solo(&profile(ATTRIBUTES), "attributes", code)?,
        [here(1, "AUnit lacks spec")]
    );
    Ok(())
}

#[test]
fn profile_attributes_an_annotation_alone_in_a_protocol_is_declared() -> Outcome {
    let codes = [
        "from typing import Protocol\n\n\nclass AUnit(Protocol):\n    spec: int\n",
        "import typing\n\n\nclass AUnit(typing.Protocol):\n    spec: int\n",
        "from typing import Protocol\n\n\nclass AUnit(Protocol[int]):\n    spec: int\n",
        "from typing import Protocol\n\n\nclass AUnit[T](Protocol):\n    spec: T\n",
        "from typing import Protocol, runtime_checkable\n\n\n@runtime_checkable\nclass AUnit(Protocol):\n    spec: int\n",
        "import typing as t\n\n\nclass AUnit(t.Protocol):\n    spec: int\n",
        "from typing import Protocol as P\n\n\nclass AUnit(P):\n    spec: int\n",
    ];
    for code in codes {
        assert_eq!(
            solo(&profile(ATTRIBUTES), "attributes", code)?,
            Vec::<String>::new(),
            "{code}"
        );
    }
    Ok(())
}

#[test]
fn profile_attributes_typing_extensions_protocol_counts_where_the_environment_has_it() -> Outcome {
    let code = "from typing_extensions import Protocol\n\n\nclass AUnit(Protocol):\n    spec: int\n";
    let site: Files<'_> = &[("typing_extensions.py", "class Protocol: ...\n")];
    let seen = foreign(&profile(ATTRIBUTES), "attributes", code, site)?;
    assert_eq!(seen.found, Vec::<String>::new());
    Ok(())
}

#[test]
fn profile_attributes_a_protocol_is_not_inherited_a_child_of_it_is_concrete() -> Outcome {
    let code = "from typing import Protocol\n\n\nclass PUnit(Protocol):\n    spec: int\n\n\nclass AUnit(PUnit): ...\n";
    assert_eq!(
        solo(&profile(ATTRIBUTES), "attributes", code)?,
        [here(8, "AUnit lacks spec")]
    );
    Ok(())
}

#[test]
fn profile_attributes_an_abstract_class_with_its_own_abstract_method_declares_by_annotation() -> Outcome {
    let code = "from abc import ABC, abstractmethod\n\n\nclass AUnit(ABC):\n    spec: int\n\n    @abstractmethod\n    def go(self): ...\n";
    assert_eq!(solo(&profile(ATTRIBUTES), "attributes", code)?, Vec::<String>::new());
    Ok(())
}

#[test]
fn profile_attributes_every_abstract_decorator_by_its_last_segment_counts() -> Outcome {
    for decorator in ABSTRACTS {
        let code = format!(
            "from abc import ABC, abstractmethod, abstractproperty, abstractclassmethod, abstractstaticmethod\nimport abc\nimport somewhere\n\n\nclass AUnit(ABC):\n    spec: int\n\n    @{decorator}\n    def go(self): ...\n"
        );
        let seen = scene(&profile(ATTRIBUTES), "attributes", &[(MODULE, &code)])?;
        assert_eq!(seen.found, Vec::<String>::new(), "@{decorator}: {}", seen.out.stdout);
    }
    Ok(())
}

#[test]
fn profile_attributes_a_decorator_that_is_not_abstract_leaves_the_class_concrete() -> Outcome {
    let code = "import functools\n\n\nclass AUnit:\n    spec: int\n\n    @functools.cache\n    def go(self): ...\n";
    assert_eq!(
        solo(&profile(ATTRIBUTES), "attributes", code)?,
        [here(4, "AUnit lacks spec")]
    );
    Ok(())
}

#[test]
fn profile_attributes_an_abstract_method_unimplemented_down_the_line_declares_by_annotation() -> Outcome {
    let code = "from abc import ABC, abstractmethod\n\n\nclass Base(ABC):\n    @abstractmethod\n    def go(self): ...\n\n\nclass AUnit(Base):\n    spec: int\n";
    assert_eq!(solo(&profile(ATTRIBUTES), "attributes", code)?, Vec::<String>::new());
    Ok(())
}

#[test]
fn profile_attributes_abc_among_the_direct_bases_declares_a_base() -> Outcome {
    let alone = "from abc import ABC\n\n\nclass AUnit(ABC):\n    spec: int\n";
    assert_eq!(solo(&profile(ATTRIBUTES), "attributes", alone)?, Vec::<String>::new());
    let dotted = "import abc\n\n\nclass AUnit(abc.ABC):\n    spec: int\n";
    assert_eq!(solo(&profile(ATTRIBUTES), "attributes", dotted)?, Vec::<String>::new());
    let mixed = "from abc import ABC\n\n\nclass Mixin: ...\n\n\nclass AUnit(Mixin, ABC):\n    spec: int\n";
    assert_eq!(solo(&profile(ATTRIBUTES), "attributes", mixed)?, Vec::<String>::new());
    Ok(())
}

#[test]
fn profile_attributes_abc_is_not_inherited_a_child_of_a_base_is_concrete() -> Outcome {
    let through = "from abc import ABC\n\n\nclass Base(ABC): ...\n\n\nclass AUnit(Base):\n    spec: int\n";
    assert_eq!(
        solo(&profile(ATTRIBUTES), "attributes", through)?,
        [here(7, "AUnit lacks spec")]
    );
    Ok(())
}

#[test]
fn profile_attributes_a_concrete_child_that_implements_the_abstract_but_never_binds_is_found() -> Outcome {
    let code = "from abc import ABC, abstractmethod\n\n\nclass RootUnit(ABC):\n    spec: int\n\n    @abstractmethod\n    def go(self): ...\n\n\nclass AUnit(RootUnit):\n    def go(self): ...\n";
    assert_eq!(
        solo(&profile(ATTRIBUTES), "attributes", code)?,
        [here(11, "AUnit lacks spec")]
    );
    Ok(())
}

#[test]
fn profile_attributes_a_concrete_child_that_binds_the_value_passes() -> Outcome {
    let code = "from abc import ABC, abstractmethod\n\n\nclass RootUnit(ABC):\n    spec: int\n\n    @abstractmethod\n    def go(self): ...\n\n\nclass AUnit(RootUnit):\n    spec = 1\n\n    def go(self): ...\n";
    assert_eq!(solo(&profile(ATTRIBUTES), "attributes", code)?, Vec::<String>::new());
    Ok(())
}

#[test]
fn profile_attributes_a_valued_binding_of_the_abstract_name_overrides_it() -> Outcome {
    let head = "from abc import ABC, abstractmethod\n\n\nclass Base(ABC):\n    @abstractmethod\n    def go(self): ...\n\n\nclass AUnit(Base):\n";
    let overridden = format!("{head}    go = None\n    spec: int\n");
    assert_eq!(
        solo(&profile(ATTRIBUTES), "attributes", &overridden)?,
        [here(9, "AUnit lacks spec")]
    );
    let open = format!("{head}    spec: int\n");
    assert_eq!(solo(&profile(ATTRIBUTES), "attributes", &open)?, Vec::<String>::new());
    Ok(())
}

#[test]
fn profile_attributes_the_nearest_class_decides_whether_a_method_is_abstract() -> Outcome {
    let implemented = "from abc import ABC, abstractmethod\n\n\nclass Base(ABC):\n    @abstractmethod\n    def go(self): ...\n\n\nclass Mid(Base):\n    def go(self): ...\n\n\nclass AUnit(Mid):\n    spec: int\n";
    assert_eq!(
        solo(&profile(ATTRIBUTES), "attributes", implemented)?,
        [here(13, "AUnit lacks spec")]
    );
    let reopened = "from abc import ABC, abstractmethod\n\n\nclass Base(ABC):\n    def go(self): ...\n\n\nclass Mid(Base):\n    @abstractmethod\n    def go(self): ...\n\n\nclass AUnit(Mid):\n    spec: int\n";
    assert_eq!(
        solo(&profile(ATTRIBUTES), "attributes", reopened)?,
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn profile_attributes_the_line_order_decides_between_a_mixin_and_an_abstract_base() -> Outcome {
    let head = "from abc import ABC, abstractmethod\n\n\nclass Base(ABC):\n    @abstractmethod\n    def go(self): ...\n\n\nclass Mixin:\n    def go(self): ...\n\n\n";
    let mixin_first = format!("{head}class AUnit(Mixin, Base):\n    spec: int\n");
    assert_eq!(
        solo(&profile(ATTRIBUTES), "attributes", &mixin_first)?,
        [here(13, "AUnit lacks spec")]
    );
    let base_first = format!("{head}class AUnit(Base, Mixin):\n    spec: int\n");
    assert_eq!(
        solo(&profile(ATTRIBUTES), "attributes", &base_first)?,
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn profile_attributes_an_abstract_method_without_abc_follows_the_ruling_literally() -> Outcome {
    let code = "from abc import abstractmethod\n\n\nclass AUnit:\n    spec: int\n\n    @abstractmethod\n    def go(self): ...\n";
    assert_eq!(solo(&profile(ATTRIBUTES), "attributes", code)?, Vec::<String>::new());
    Ok(())
}

#[test]
fn profile_attributes_through_a_bank_abc_with_no_environment() -> Outcome {
    let head = "from contextlib import AbstractContextManager\n\n\n";
    let open = format!("{head}class AUnit(AbstractContextManager):\n    spec: int\n");
    let seen = scene(&profile(ATTRIBUTES), "attributes", &[(MODULE, &open)])?;
    assert_eq!(seen.found, Vec::<String>::new());
    assert_eq!(
        seen.census.as_deref(),
        Some("profile: 1 classes judged, 0 skipped (unresolved base)")
    );
    let closed =
        format!("{head}class AUnit(AbstractContextManager):\n    spec: int\n\n    def __exit__(self, *exc): ...\n");
    assert_eq!(
        solo(&profile(ATTRIBUTES), "attributes", &closed)?,
        [here(4, "AUnit lacks spec")]
    );
    Ok(())
}

#[test]
fn profile_attributes_real_stdlib_abstract_class_resolves_through_the_interpreter() -> Outcome {
    let code = "import collections.abc\n\n\nclass AUnit(collections.abc.Iterator):\n    spec: int\n";
    let Some(seen) = real(&profile(ATTRIBUTES), "attributes", code)? else {
        return Ok(());
    };
    assert_eq!(seen.found, Vec::<String>::new());
    let done = "import collections.abc\n\n\nclass AUnit(collections.abc.Iterator):\n    spec: int\n\n    def __next__(self): ...\n";
    let Some(seen) = real(&profile(ATTRIBUTES), "attributes", done)? else {
        return Ok(());
    };
    assert_eq!(seen.found, [here(4, "AUnit lacks spec")]);
    Ok(())
}

///// line and never /////

#[test]
fn profile_line_a_protocol_passes_and_a_plain_class_fails() -> Outcome {
    let profiles = profile(LINE);
    let code = "from typing import Protocol\n\n\nclass AUnit(Protocol): ...\n\n\nclass BUnit: ...\n\n\nclass CUnit(AUnit): ...\n";
    assert_eq!(
        solo(&profiles, "line", code)?,
        [here(7, "BUnit is not a typing.Protocol")]
    );
    Ok(())
}

#[test]
fn profile_line_names_the_alternatives_and_any_one_of_them_satisfies() -> Outcome {
    let profiles = profile("  line: [typing.Protocol, pkg.base.Root]\n");
    let seen = scene(
        &profiles,
        "line",
        &[
            ("app/pkg/base.py", "class Root: ...\n"),
            (
                MODULE,
                "from pkg.base import Root\n\n\nclass AUnit(Root): ...\n\n\nclass BUnit: ...\n",
            ),
        ],
    )?;
    assert_eq!(seen.found, [here(7, "BUnit is not a typing.Protocol or pkg.base.Root")]);
    Ok(())
}

#[test]
fn profile_line_every_written_form_of_the_protocol_resolves_to_its_dotted_name() -> Outcome {
    let codes = [
        "import typing\n\n\nclass AUnit(typing.Protocol): ...\n",
        "import typing as t\n\n\nclass AUnit(t.Protocol): ...\n",
        "from typing import Protocol as P\n\n\nclass AUnit(P): ...\n",
        "from typing import Protocol\n\n\nclass AUnit(Protocol[int]): ...\n",
        "from typing import Protocol\n\n\nclass AUnit[T](Protocol): ...\n",
        "from typing import Protocol\n\n\nclass Mid(Protocol): ...\n\n\nclass AUnit(Mid): ...\n",
    ];
    for code in codes {
        assert_eq!(solo(&profile(LINE), "line", code)?, Vec::<String>::new(), "{code}");
    }
    Ok(())
}

#[test]
fn profile_line_an_unresolved_base_is_not_judged_unless_the_line_is_already_written() -> Outcome {
    let unknown = "from nowhere import Base\n\n\nclass AUnit(Base): ...\n";
    let seen = scene(&profile(LINE), "line", &[(MODULE, unknown)])?;
    assert_eq!(seen.found, Vec::<String>::new());
    assert_eq!(
        seen.census.as_deref(),
        Some("profile: 0 classes judged, 1 skipped (unresolved base)")
    );
    let written = "from typing import Protocol\nfrom nowhere import Base\n\n\nclass AUnit(Base, Protocol): ...\n";
    assert_eq!(solo(&profile(LINE), "line", written)?, Vec::<String>::new());
    Ok(())
}

#[test]
fn profile_line_the_resolved_part_of_a_broken_line_of_an_ancestor_satisfies_it() -> Outcome {
    let code = "from typing import Protocol\nfrom nowhere import Base\n\n\nclass Mid(Base, Protocol): ...\n\n\nclass AUnit(Mid): ...\n";
    let seen = scene(&profile(LINE), "line", &[(MODULE, code)])?;
    assert_eq!(seen.found, Vec::<String>::new(), "{}", seen.out.stdout);
    Ok(())
}

#[test]
fn profile_methods_and_attributes_are_not_judged_behind_a_broken_line() -> Outcome {
    let code = "from nowhere import Base\n\n\nclass AUnit(Base): ...\n";
    for (extra, aspect) in [(METHODS, "methods"), (ATTRIBUTES, "attributes")] {
        let seen = scene(&profile(extra), aspect, &[(MODULE, code)])?;
        assert_eq!(seen.found, Vec::<String>::new(), "{aspect}");
        assert_eq!(
            seen.census.as_deref(),
            Some("profile: 0 classes judged, 1 skipped (unresolved base)"),
            "{aspect}"
        );
    }
    Ok(())
}

#[test]
fn profile_methods_stay_unjudged_though_a_resolved_part_of_the_line_is_written() -> Outcome {
    let code = "from abc import ABC\nfrom nowhere import Base\n\n\nclass AUnit(Base, ABC): ...\n";
    let seen = scene(&profile(METHODS), "methods", &[(MODULE, code)])?;
    assert_eq!(seen.found, Vec::<String>::new());
    assert_eq!(
        seen.census.as_deref(),
        Some("profile: 0 classes judged, 1 skipped (unresolved base)")
    );
    Ok(())
}

#[test]
fn profile_never_a_class_of_the_forbidden_line_is_found_at_its_name() -> Outcome {
    let profiles = profile(NEVER);
    let code = "from abc import ABC\n\n\nclass AUnit(ABC): ...\n\n\nclass BUnit: ...\n\n\nclass Base(ABC): ...\n\n\nclass CUnit(Base): ...\n";
    let seen = scene(&profiles, "never", &[(MODULE, code)])?;
    assert_eq!(
        seen.found,
        [here(4, "AUnit is a abc.ABC"), here(13, "CUnit is a abc.ABC")]
    );
    assert!(
        seen.out.stdout.starts_with(&format!("{MODULE}:4:7: t.p")),
        "{}",
        seen.out.stdout
    );
    Ok(())
}

#[test]
fn profile_never_every_written_form_of_the_forbidden_name_is_found() -> Outcome {
    let codes = [
        "import abc\n\n\nclass AUnit(abc.ABC): ...\n",
        "import abc as a\n\n\nclass AUnit(a.ABC): ...\n",
        "from abc import ABC as Root\n\n\nclass AUnit(Root): ...\n",
        "from abc import ABC\n\n\nclass AUnit[T](ABC): ...\n",
    ];
    for code in codes {
        assert_eq!(solo(&profile(NEVER), "never", code)?.len(), 1, "{code}");
    }
    Ok(())
}

#[test]
fn profile_never_a_base_known_as_written_decides_though_the_line_is_broken() -> Outcome {
    let code =
        "from abc import ABC\nfrom nowhere import Base\n\n\nclass AUnit(Base, ABC): ...\n\n\nclass BUnit(Base): ...\n";
    let seen = scene(&profile(NEVER), "never", &[(MODULE, code)])?;
    assert_eq!(seen.found, [here(5, "AUnit is a abc.ABC")]);
    assert_eq!(
        seen.census.as_deref(),
        Some("profile: 0 classes judged, 2 skipped (unresolved base)")
    );
    Ok(())
}

#[test]
fn profile_never_a_forbidden_base_behind_a_broken_line_of_an_ancestor() -> Outcome {
    let code =
        "from abc import ABC\nfrom nowhere import Base\n\n\nclass Mid(Base, ABC): ...\n\n\nclass AUnit(Mid): ...\n";
    let seen = scene(&profile(NEVER), "never", &[(MODULE, code)])?;
    assert_eq!(seen.found, [here(8, "AUnit is a abc.ABC")], "{}", seen.out.stdout);
    Ok(())
}

#[test]
fn profile_never_an_unrelated_unresolved_base_is_not_judged() -> Outcome {
    let code = "from nowhere import Base\n\n\nclass AUnit(Base): ...\n";
    let seen = scene(&profile(NEVER), "never", &[(MODULE, code)])?;
    assert_eq!(seen.found, Vec::<String>::new());
    assert_eq!(
        seen.census.as_deref(),
        Some("profile: 0 classes judged, 1 skipped (unresolved base)")
    );
    Ok(())
}

#[test]
fn profile_never_an_environment_class_deep_in_the_line_is_found() -> Outcome {
    let site: Files<'_> = &[("foreignlib.py", "from abc import ABC\n\n\nclass Base(ABC): ...\n")];
    let code = "from foreignlib import Base\n\n\nclass AUnit(Base): ...\n";
    let seen = foreign(&profile(NEVER), "never", code, site)?;
    assert_eq!(seen.found, [here(4, "AUnit is a abc.ABC")]);
    Ok(())
}

///// bases /////

const NOT_PROTOCOL: &str = "AUnit does not name typing.Protocol among its bases";

fn refusal(line: u32) -> Vec<String> {
    vec![here(line, NOT_PROTOCOL)]
}

#[test]
fn profile_bases_every_written_form_of_the_protocol_passes_and_its_one_token_mutant_is_found() -> Outcome {
    let pairs = [
        (
            "from typing import Protocol\n\n\nclass AUnit(Protocol): ...\n",
            "from typing import Protocol\n\n\nclass AUnit: ...\n",
        ),
        (
            "import typing\n\n\nclass AUnit(typing.Protocol): ...\n",
            "import typing\n\n\nclass AUnit(typing.Sized): ...\n",
        ),
        (
            "from typing import Protocol as P\n\n\nclass AUnit(P): ...\n",
            "from typing import Sized as P\n\n\nclass AUnit(P): ...\n",
        ),
        (
            "from typing import Protocol\n\n\nclass AUnit(Protocol[int]): ...\n",
            "from typing import Sized\n\n\nclass AUnit(Sized[int]): ...\n",
        ),
        (
            "from typing import Protocol\n\n\nclass AUnit[T](Protocol[T]): ...\n",
            "from typing import Protocol\n\n\nclass AUnit[T](list[T]): ...\n",
        ),
        (
            "import typing as t\n\n\nclass AUnit(t.Protocol): ...\n",
            "import typing as t\n\n\nclass AUnit(t.Sized): ...\n",
        ),
    ];
    for (accepted, mutant) in pairs {
        assert_eq!(
            solo(&profile(BASES), "bases", accepted)?,
            Vec::<String>::new(),
            "{accepted}"
        );
        assert_eq!(solo(&profile(BASES), "bases", mutant)?, refusal(4), "{mutant}");
    }
    Ok(())
}

#[test]
fn profile_bases_typing_extensions_protocol_passes_only_where_the_profile_lists_it() -> Outcome {
    let code = "from typing_extensions import Protocol\n\n\nclass AUnit(Protocol): ...\n";
    assert_eq!(solo(&profile(BASES_BOTH), "bases", code)?, Vec::<String>::new());
    assert_eq!(solo(&profile(BASES), "bases", code)?, refusal(4));
    let both = solo(&profile(BASES_BOTH), "bases", "class AUnit: ...\n")?;
    assert_eq!(
        both,
        [here(
            1,
            "AUnit does not name typing.Protocol or typing_extensions.Protocol among its bases"
        )]
    );
    Ok(())
}

#[test]
fn profile_bases_a_protocol_only_inherited_through_a_same_file_parent_is_found() -> Outcome {
    let inherited = "from typing import Protocol\n\n\nclass Mid(Protocol): ...\n\n\nclass AUnit(Mid): ...\n";
    assert_eq!(solo(&profile(BASES), "bases", inherited)?, refusal(7));
    let written = "from typing import Protocol\n\n\nclass Mid(Protocol): ...\n\n\nclass AUnit(Mid, Protocol): ...\n";
    assert_eq!(solo(&profile(BASES), "bases", written)?, Vec::<String>::new());
    Ok(())
}

#[test]
fn profile_bases_a_protocol_only_inherited_through_an_imported_project_parent_is_found() -> Outcome {
    let parent = (
        "app/pkg/base.py",
        "from typing import Protocol\n\n\nclass Root(Protocol): ...\n",
    );
    let inherited = "from pkg.base import Root\n\n\nclass AUnit(Root): ...\n";
    let seen = scene(&profile(BASES), "bases", &[parent, (MODULE, inherited)])?;
    assert_eq!(seen.found, refusal(4));
    let written = "from typing import Protocol\nfrom pkg.base import Root\n\n\nclass AUnit(Root, Protocol): ...\n";
    let seen = scene(&profile(BASES), "bases", &[parent, (MODULE, written)])?;
    assert_eq!(seen.found, Vec::<String>::new());
    Ok(())
}

#[test]
fn profile_bases_a_class_with_no_bases_or_on_abc_is_found() -> Outcome {
    let bare = "class AUnit: ...\n";
    let abc = "from abc import ABC\n\n\nclass AUnit(ABC): ...\n";
    assert_eq!(solo(&profile(BASES), "bases", bare)?, refusal(1));
    assert_eq!(solo(&profile(BASES), "bases", abc)?, refusal(4));
    let both = "from abc import ABC\nfrom typing import Protocol\n\n\nclass AUnit(ABC, Protocol): ...\n";
    assert_eq!(solo(&profile(BASES), "bases", both)?, Vec::<String>::new());
    Ok(())
}

#[test]
fn profile_bases_a_local_class_named_protocol_is_not_typings() -> Outcome {
    let local = "class Protocol: ...\n\n\nclass AUnit(Protocol): ...\n";
    assert_eq!(solo(&profile(BASES), "bases", local)?, refusal(4));
    let foreign = "from elsewhere import Protocol\n\n\nclass AUnit(Protocol): ...\n";
    assert_eq!(solo(&profile(BASES), "bases", foreign)?, refusal(4));
    let typing = "from typing import Protocol\n\n\nclass AUnit(Protocol): ...\n";
    assert_eq!(solo(&profile(BASES), "bases", typing)?, Vec::<String>::new());
    Ok(())
}

#[test]
fn profile_bases_a_class_of_the_home_not_ending_in_the_suffix_is_left_alone() -> Outcome {
    let code = "class Row: ...\n\n\nclass AUnit: ...\n\n\nclass Helper(Missing): ...\n";
    assert_eq!(solo(&profile(BASES), "bases", code)?, refusal(4));
    Ok(())
}

#[test]
fn profile_bases_is_judged_though_the_rest_of_the_line_is_unresolved_and_leaves_the_census_alone() -> Outcome {
    let written = "from typing import Protocol\nfrom nowhere import Missing\n\n\nclass AUnit(Missing, Protocol): ...\n";
    let seen = scene(&profile(BASES), "bases", &[(MODULE, written)])?;
    assert_eq!(seen.found, Vec::<String>::new());
    let unwritten = "from nowhere import Missing\n\n\nclass AUnit(Missing): ...\n";
    let seen = scene(&profile(BASES), "bases", &[(MODULE, unwritten)])?;
    assert_eq!(seen.found, refusal(4));
    assert_eq!(
        seen.census.as_deref(),
        Some("profile: 0 classes judged, 0 skipped (unresolved base)")
    );
    let line = scene(&profile(LINE), "line", &[(MODULE, unwritten)])?;
    assert_eq!(
        line.census.as_deref(),
        Some("profile: 0 classes judged, 1 skipped (unresolved base)")
    );
    Ok(())
}

#[test]
fn profile_bases_rule_on_a_profile_without_bases_is_unfed() -> Outcome {
    let out = refused(HOME, &rule("unit", "bases", "{}"))?;
    assert_eq!(
        out.stderr,
        "cat/rules/r.yml: rule `t.p` judges `bases`, which profile `unit` gives no data for\n"
    );
    let line_only = refused(&profile(LINE), &rule("unit", "bases", "{}"))?;
    assert!(line_only.stderr.contains("judges `bases`"), "{}", line_only.stderr);
    Ok(())
}

#[test]
fn profile_bases_rule_renders_its_aspect() -> Outcome {
    let project = world(&profile(BASES), &rule("unit", "bases", "{}"))?;
    let text = project.run(&["rule", "t.p"])?;
    assert!(text.stdout.contains("engine: profile unit bases\n"), "{}", text.stdout);
    let json = project.run(&["rule", "t.p", "--output-format", "json"])?;
    let value: serde_json::Value = serde_json::from_str(&json.stdout)?;
    assert_eq!(
        value["check"],
        serde_json::json!({"engine": "profile", "kinds": ["unit"], "aspect": "bases"})
    );
    Ok(())
}

///// census /////

#[test]
fn profile_census_counts_entity_classes_only_and_only_for_a_lineal_aspect() -> Outcome {
    let code = "class Row: ...\n\n\nclass AUnit: ...\n\n\nclass BUnit(AUnit): ...\n";
    let seen = scene(&profile(METHODS), "methods", &[(MODULE, code)])?;
    assert_eq!(
        seen.census.as_deref(),
        Some("profile: 2 classes judged, 0 skipped (unresolved base)")
    );
    let surname = scene(&profile(METHODS), "surname", &[(MODULE, code)])?;
    assert_eq!(
        surname.census.as_deref(),
        Some("profile: 0 classes judged, 0 skipped (unresolved base)")
    );
    Ok(())
}

#[test]
fn profile_census_is_silent_without_verbose_and_without_a_profile_rule() -> Outcome {
    let project = world(&profile(METHODS), &rule("unit", "methods", "{}"))?;
    project.file(MODULE, "class AUnit: ...\n")?;
    let quiet = project.check(&[])?;
    assert!(!quiet.stderr.contains("profile:"), "{}", quiet.stderr);
    let other = Project::with_rules("[\"app\"]", common::ELIF)?;
    other.file(MODULE, "class AUnit: ...\n")?;
    let verbose = other.check(&["-v"])?;
    assert!(!verbose.stderr.contains("profile:"), "{}", verbose.stderr);
    Ok(())
}

#[test]
fn profile_census_adds_up_across_modules_and_rules() -> Outcome {
    let rules = format!(
        "{}{}",
        rule("unit", "methods", "{}"),
        rule("unit", "never", "{}").replace("t.p", "t.q")
    );
    let project = world(&profile(&format!("{METHODS}{NEVER}")), &rules)?;
    project.file("app/pkg/u/a.py", "class AUnit: ...\n")?;
    project.file(
        "app/pkg/u/b.py",
        "from nowhere import Base\n\n\nclass BUnit(Base): ...\n",
    )?;
    let seen = read(project.check(&["-v"])?);
    assert_eq!(
        seen.census.as_deref(),
        Some("profile: 1 classes judged, 1 skipped (unresolved base)")
    );
    Ok(())
}

///// scope, kinds, homes /////

#[test]
fn profile_scope_exclude_and_include_narrow_a_rule_inside_the_home() -> Outcome {
    let files: Files<'_> = &[
        ("app/pkg/u/a.py", "class Row: ...\n"),
        ("app/pkg/u/models.py", "class Row: ...\n"),
    ];
    let excluded = world(&profile(""), &rule("unit", "surname", "{exclude: [\"**/models.py\"]}"))?;
    files
        .iter()
        .try_for_each(|(path, text)| excluded.file(path, text).map(drop))?;
    assert_eq!(
        read(excluded.check(&[])?).found,
        ["app/pkg/u/a.py:1 Row does not end in Unit"]
    );
    let included = world(&profile(""), &rule("unit", "surname", "{include: [\"**/models.py\"]}"))?;
    files
        .iter()
        .try_for_each(|(path, text)| included.file(path, text).map(drop))?;
    assert_eq!(
        read(included.check(&[])?).found,
        ["app/pkg/u/models.py:1 Row does not end in Unit"]
    );
    Ok(())
}

#[test]
fn profile_home_a_module_outside_every_home_is_never_judged() -> Outcome {
    let seen = scene(
        &profile(""),
        "surname",
        &[
            ("app/pkg/other/m.py", "class Row: ...\n"),
            ("app/pkg/u/deep/m.py", "class Row: ...\n"),
        ],
    )?;
    assert_eq!(seen.found, Vec::<String>::new());
    Ok(())
}

#[test]
fn profile_a_rule_over_two_kinds_judges_each_module_by_its_own_profile() -> Outcome {
    let profiles = "- kind: unit\n  home: [\"app/pkg/u/*.py\"]\n  suffix: Unit\n- kind: part\n  home: [\"app/pkg/p/*.py\"]\n  suffix: Part\n";
    let project = world(profiles, &rule("unit, part", "surname", "{}"))?;
    project.file("app/pkg/u/m.py", "class AUnit: ...\n\n\nclass Row: ...\n")?;
    project.file("app/pkg/p/m.py", "class APart: ...\n\n\nclass AUnit: ...\n")?;
    let seen = read(project.check(&[])?);
    assert_eq!(
        seen.found,
        [
            "app/pkg/p/m.py:4 AUnit does not end in Part",
            "app/pkg/u/m.py:4 Row does not end in Unit"
        ]
    );
    Ok(())
}

#[test]
fn profile_a_module_in_two_homes_is_judged_by_both_profiles() -> Outcome {
    let profiles = "- kind: unit\n  home: [\"app/pkg/u/*.py\"]\n  suffix: Unit\n- kind: part\n  home: [\"app/pkg/u/*.py\"]\n  suffix: Part\n";
    let project = world(profiles, &rule("unit, part", "surname", "{}"))?;
    project.file(MODULE, "class AUnit: ...\n")?;
    let seen = read(project.check(&[])?);
    assert_eq!(seen.found, [here(1, "AUnit does not end in Part")]);
    Ok(())
}

///// faults /////

fn refused(profiles: &str, rules: &str) -> Result<Out, Fault> {
    let out = world(profiles, rules)?.check(&[])?;
    assert_eq!((out.stdout.as_str(), out.code), ("", 2), "{out:?}");
    Ok(out)
}

#[test]
fn profile_fault_an_aspect_the_profile_gives_no_data_for_is_unfed() -> Outcome {
    for aspect in ["present", "surname", "methods", "attributes", "line", "bases", "never"] {
        let bare = "- kind: unit\n  home: [\"app/pkg/u/*.py\"]\n";
        let out = refused(bare, &rule("unit", aspect, "{}"))?;
        assert_eq!(
            out.stderr,
            format!("cat/rules/r.yml: rule `t.p` judges `{aspect}`, which profile `unit` gives no data for\n"),
            "{aspect}"
        );
    }
    Ok(())
}

#[test]
fn profile_fault_an_empty_list_feeds_nothing_and_a_fed_aspect_loads() -> Outcome {
    let empty = "- kind: unit\n  home: [\"app/pkg/u/*.py\"]\n  methods: []\n  attributes: []\n  line: []\n  bases: []\n  never: []\n";
    for aspect in ["methods", "attributes", "line", "bases", "never"] {
        refused(empty, &rule("unit", aspect, "{}"))?;
    }
    let fed = world(
        &profile(&format!("{METHODS}{ATTRIBUTES}{LINE}{BASES}{NEVER}")),
        &rule("unit", "never", "{}"),
    )?;
    fed.dir("app/pkg/u")?;
    let out = fed.check(&[])?;
    assert_eq!(
        (out.stdout.as_str(), out.stderr.as_str(), out.code),
        ("All checks passed.\n", "", 0)
    );
    Ok(())
}

#[test]
fn profile_fault_one_unfed_kind_of_two_is_named() -> Outcome {
    let profiles =
        "- kind: unit\n  home: [\"app/pkg/u/*.py\"]\n  methods: [go]\n- kind: part\n  home: [\"app/pkg/p/*.py\"]\n";
    let out = refused(profiles, &rule("unit, part", "methods", "{}"))?;
    assert_eq!(
        out.stderr,
        "cat/rules/r.yml: rule `t.p` judges `methods`, which profile `part` gives no data for\n"
    );
    Ok(())
}

#[test]
fn profile_fault_a_kind_no_profile_declares_is_unknown() -> Outcome {
    let out = refused(&profile(""), &rule("ghost", "present", "{}"))?;
    assert_eq!(out.stderr, "cat/rules/r.yml: no profile declares kind `ghost`\n");
    Ok(())
}

#[test]
fn profile_fault_a_rule_with_no_kinds_is_refused_at_its_line() -> Outcome {
    let out = refused(&profile(""), &rule("", "present", "{}"))?;
    assert_eq!(
        out.stderr,
        "cat/rules/r.yml:4: empty-kinds: a profile check names at least one kind\n"
    );
    Ok(())
}

#[test]
fn profile_fault_unknown_aspect_unknown_field_and_missing_aspect() -> Outcome {
    let aspect = rule("unit", "shape", "{}");
    assert!(
        refused(&profile(""), &aspect)?
            .stderr
            .contains("unknown variant `shape`")
    );
    let field = rule("unit", "present", "{}").replace("aspect: present", "aspect: present, extra: 1");
    assert!(refused(&profile(""), &field)?.stderr.contains("unknown field `extra`"));
    let missing = rule("unit", "present", "{}").replace(", aspect: present", "");
    assert!(
        refused(&profile(""), &missing)?
            .stderr
            .contains("missing field `aspect`")
    );
    Ok(())
}

#[test]
fn profile_fault_unknown_profile_field_and_missing_home_are_refused() -> Outcome {
    let unknown = format!("{HOME}  lines: 3\n");
    assert!(
        refused(&unknown, &rule("unit", "present", "{}"))?
            .stderr
            .contains("unknown field `lines`")
    );
    let homeless = "- kind: unit\n  suffix: Unit\n";
    assert!(
        refused(homeless, &rule("unit", "present", "{}"))?
            .stderr
            .contains("missing field `home`")
    );
    let loud = format!("{HOME}  suffix: Again\n");
    assert_eq!(world(&loud, &rule("unit", "present", "{}"))?.check(&[])?.code, 2);
    Ok(())
}

#[test]
fn profile_fault_a_kind_declared_twice_is_refused() -> Outcome {
    let twice = format!("{HOME}{HOME}");
    let out = refused(&twice, &rule("unit", "present", "{}"))?;
    assert!(out.stderr.contains("unit"), "{}", out.stderr);
    Ok(())
}

///// thresholds /////

fn thresholds(declared: &str, overrides: &str) -> Result<Out, Fault> {
    let project = world(
        &profile(&format!("  thresholds: {declared}\n")),
        &rule("unit", "present", "{}"),
    )?;
    project.config(&format!("roots = [\"app\"]\ncatalogues = [\"cat/\"]\n{overrides}"))?;
    project.file(MODULE, "class AUnit: ...\n")?;
    project.check(&[])
}

#[test]
fn profile_thresholds_override_of_a_declared_name_is_accepted() -> Outcome {
    let out = thresholds(
        "{lines: 400, depth: 3}",
        "[profiles.unit.thresholds]\nlines = 120\ndepth = 2\n",
    )?;
    assert_eq!(
        (out.stdout.as_str(), out.stderr.as_str(), out.code),
        ("All checks passed.\n", "", 0)
    );
    Ok(())
}

#[test]
fn profile_thresholds_override_of_an_undeclared_name_is_refused() -> Outcome {
    let out = thresholds(
        "{lines: 400}",
        "[profiles.unit.thresholds]\nlines = 120\nbranches = 9\n",
    )?;
    assert_eq!(
        (out.stdout.as_str(), out.stderr.as_str(), out.code),
        ("", "law.toml: profile `unit` declares no threshold `branches`\n", 2)
    );
    Ok(())
}

#[test]
fn profile_thresholds_override_on_a_profile_declaring_none_or_a_ghost_kind_is_refused() -> Outcome {
    let none = thresholds("{}", "[profiles.unit.thresholds]\nlines = 1\n")?;
    assert_eq!(none.stderr, "law.toml: profile `unit` declares no threshold `lines`\n");
    let ghost = thresholds("{lines: 1}", "[profiles.ghost.thresholds]\nlines = 1\n")?;
    assert!(
        ghost.stderr.contains("no profile declares kind `ghost`"),
        "{}",
        ghost.stderr
    );
    assert_eq!(ghost.code, 2);
    Ok(())
}

#[test]
fn profile_thresholds_a_value_that_is_no_count_is_refused() -> Outcome {
    for value in ["\"many\"", "-1", "1.5"] {
        let out = thresholds("{lines: 1}", &format!("[profiles.unit.thresholds]\nlines = {value}\n"))?;
        assert_eq!(out.code, 2, "{value}: {out:?}");
    }
    Ok(())
}

///// rule and test /////

#[test]
fn profile_rule_renders_its_engine_kinds_and_aspect() -> Outcome {
    let project = world(&profile(""), &rule("unit", "surname", "{}"))?;
    let text = project.run(&["rule", "t.p"])?;
    assert!(
        text.stdout.contains("engine: profile unit surname\n"),
        "{}",
        text.stdout
    );
    assert_eq!(text.code, 0);
    let json = project.run(&["rule", "t.p", "--output-format", "json"])?;
    let value: serde_json::Value = serde_json::from_str(&json.stdout)?;
    assert_eq!(
        value["check"],
        serde_json::json!({"engine": "profile", "kinds": ["unit"], "aspect": "surname"})
    );
    Ok(())
}

fn proven(profiles: &str, rules: &str) -> Result<Out, Fault> {
    world(profiles, rules)?.run(&["test"])
}

const CASES: &str = "  cases:\n    valid:\n      - path: app/pkg/u/m.py\n        code: \"class AUnit:\\n    spec = 1\\n\"\n    invalid:\n      - path: app/pkg/u/m.py\n        code: \"class AUnit: ...\\n\"\n";

#[test]
fn profile_test_cases_in_a_home_pass_when_each_is_right() -> Outcome {
    let out = proven(
        &profile(ATTRIBUTES),
        &format!("{}{CASES}", rule("unit", "attributes", "{}")),
    )?;
    assert_eq!(
        (out.stdout.as_str(), out.code),
        ("Tested 1 rule, 2 cases: 0 failed, 0 untested.\n", 0),
        "{out:?}"
    );
    Ok(())
}

#[test]
fn profile_test_swapped_cases_each_fail_for_their_own_reason() -> Outcome {
    let swapped = CASES
        .replace("AUnit:\\n    spec = 1\\n", "TMP")
        .replace("AUnit: ...\\n", "AUnit:\\n    spec = 1\\n")
        .replace("TMP", "AUnit: ...\\n");
    let out = proven(
        &profile(ATTRIBUTES),
        &format!("{}{swapped}", rule("unit", "attributes", "{}")),
    )?;
    assert_eq!(
        out.stdout,
        "t.p: valid case 1: expected no finding, found 1 (the first at 1:7)\nt.p: invalid case 1: expected a finding, found none\n\nTested 1 rule, 2 cases: 2 failed, 0 untested.\n"
    );
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn profile_test_a_same_file_base_resolves_inside_a_case() -> Outcome {
    let cases = "  cases:\n    valid:\n      - path: app/pkg/u/m.py\n        code: \"class Base:\\n    def go(self): ...\\n\\n\\nclass AUnit(Base): ...\\n\"\n    invalid:\n      - path: app/pkg/u/n.py\n        code: \"class Base:\\n    def other(self): ...\\n\\n\\nclass AUnit(Base): ...\\n\"\n";
    let out = proven(&profile(METHODS), &format!("{}{cases}", rule("unit", "methods", "{}")))?;
    assert_eq!(out.code, 0, "{out:?}");
    Ok(())
}

#[test]
fn profile_test_invalid_cases_all_outside_every_home_fail_the_rule() -> Outcome {
    let homeless = CASES.replace("app/pkg/u/m.py", "app/pkg/elsewhere/m.py");
    let out = proven(
        &profile(ATTRIBUTES),
        &format!("{}{homeless}", rule("unit", "attributes", "{}")),
    )?;
    assert_eq!(
        out.stdout,
        "t.p: invalid case 1: expected a finding, found none\nt.p: invalid: every case sits outside the homes of its kinds: an unproven rule is not a rule\n\nTested 1 rule, 2 cases: 2 failed, 0 untested.\n"
    );
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn profile_test_a_case_in_the_home_beside_one_outside_is_enough() -> Outcome {
    let mixed = CASES.replace(
        "      - path: app/pkg/u/m.py\n        code: \"class AUnit:\\n    spec = 1",
        "      - path: app/pkg/elsewhere/m.py\n        code: \"class AUnit:\\n    spec = 1",
    );
    let out = proven(
        &profile(ATTRIBUTES),
        &format!("{}{mixed}", rule("unit", "attributes", "{}")),
    )?;
    assert_eq!(out.code, 0, "{out:?}");
    Ok(())
}

#[test]
fn profile_test_cases_sharing_a_path_do_not_share_their_bases() -> Outcome {
    let cases = "  cases:\n    valid:\n      - path: app/pkg/u/m.py\n        code: \"class Base:\\n    def go(self): ...\\n\\n\\nclass AUnit(Base): ...\\n\"\n    invalid:\n      - path: app/pkg/u/m.py\n        code: \"class Base:\\n    def other(self): ...\\n\\n\\nclass AUnit(Base): ...\\n\"\n";
    let out = proven(&profile(METHODS), &format!("{}{cases}", rule("unit", "methods", "{}")))?;
    assert_eq!(
        (out.stdout.as_str(), out.code),
        ("Tested 1 rule, 2 cases: 0 failed, 0 untested.\n", 0),
        "the invalid case's Base lacks go, and is judged against the valid case's Base"
    );
    Ok(())
}

///// loose ends /////

#[test]
fn profile_loose_ends_every_crate_manifest_has_a_one_line_description() -> Outcome {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let out = Command::new(cargo)
        .args(["metadata", "--no-deps", "--offline", "--format-version", "1"])
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output()?;
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let metadata: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    let packages = metadata["packages"].as_array().ok_or("no packages")?;
    assert!(packages.len() >= 7, "{} packages", packages.len());
    for package in packages {
        let name = package["name"].as_str().unwrap_or_default();
        let description = package["description"].as_str().unwrap_or_default();
        assert!(
            !description.trim().is_empty() && !description.contains('\n'),
            "{name}: {description:?}"
        );
    }
    Ok(())
}

///// the bank /////

#[test]
fn profile_bank_answers_only_when_no_root_holds_the_module() -> Outcome {
    let code =
        "from contextlib import AbstractContextManager\n\n\nclass AUnit(AbstractContextManager):\n    spec: int\n";
    let bank = scene(&profile(ATTRIBUTES), "attributes", &[(MODULE, code)])?;
    assert_eq!(bank.found, Vec::<String>::new());
    let shadowed = scene(
        &profile(ATTRIBUTES),
        "attributes",
        &[
            (
                "app/contextlib.py",
                "class AbstractContextManager:\n    def __exit__(self, *exc): ...\n",
            ),
            (MODULE, code),
        ],
    )?;
    assert_eq!(shadowed.found, [here(4, "AUnit lacks spec")]);
    Ok(())
}

#[test]
fn profile_bank_typed_dict_and_named_tuple_stay_unresolved_without_an_environment() -> Outcome {
    for base in ["TypedDict", "NamedTuple"] {
        let code = format!("from typing import {base}\n\n\nclass AUnit({base}):\n    spec: int\n");
        let seen = scene(&profile(ATTRIBUTES), "attributes", &[(MODULE, &code)])?;
        assert_eq!(seen.found, Vec::<String>::new(), "{base}");
        assert_eq!(
            seen.census.as_deref(),
            Some("profile: 0 classes judged, 1 skipped (unresolved base)"),
            "{base}"
        );
    }
    Ok(())
}

#[test]
fn profile_bank_enum_and_dataclass_bases_resolve_with_no_environment() -> Outcome {
    let code = "from enum import StrEnum\n\n\nclass AUnit(StrEnum):\n    ONE = \"one\"\n";
    let seen = scene(&profile(METHODS), "methods", &[(MODULE, code)])?;
    assert_eq!(seen.found, [here(4, "AUnit lacks go")]);
    assert_eq!(
        seen.census.as_deref(),
        Some("profile: 1 classes judged, 0 skipped (unresolved base)")
    );
    Ok(())
}
