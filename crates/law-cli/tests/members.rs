mod common;

use common::{Out, Outcome, Project};

const MODEL: &str = "field_validator, model_validator";
const LINEAGE: &str = "lineage: {common: [common]}";
const ORDER: &str = "order: privates-first";
const BOTH: &str = "lineage: {common: [common]}, order: privates-first";
const SHARED: &str = "lineage: {common: [shared]}";

type Files<'a> = &'a [(&'a str, &'a str)];
type Fault = Box<dyn std::error::Error>;

/// A fake interpreter: a standard library its `pyvenv.cfg` points at, and a site-packages.
struct Env<'a> {
    stdlib: Files<'a>,
    site: Files<'a>,
}

/// What a run said: its findings as `path:line detail`, its stderr census lines, its exit code.
struct Seen {
    found: Vec<String>,
    notes: Vec<String>,
    stderr: String,
    code: i32,
}

fn rule(spec: &str, scope: &str) -> String {
    format!(
        "- id: t.members\n  message: genealogy\n  scope: {{include: [\"{scope}\"]}}\n  check: {{engine: members, {spec}}}\n  fix: {{safety: none, reason: x}}\n"
    )
}

fn venv(project: &Project, env: &Env<'_>) -> Result<(), Fault> {
    let root = project.path().display();
    project.file(
        ".venv/pyvenv.cfg",
        &format!("home = {root}/py/bin\nversion_info = 3.14.0.final.0\n"),
    )?;
    project.dir("py/lib/python3.14")?;
    project.dir(".venv/lib/python3.14/site-packages")?;
    for (path, text) in env.stdlib {
        project.file(&format!("py/lib/python3.14/{path}"), text)?;
    }
    for (path, text) in env.site {
        project.file(&format!(".venv/lib/python3.14/site-packages/{path}"), text)?;
    }
    Ok(())
}

fn seen(out: Out) -> Seen {
    let marker = " t.members genealogy: ";
    let found = out
        .stdout
        .lines()
        .filter_map(|line| {
            let (place, detail) = line.split_once(marker)?;
            let mut parts = place.trim_end_matches(':').rsplitn(3, ':');
            let (_, line, path) = (parts.next()?, parts.next()?, parts.next()?);
            Some(format!("{path}:{line} {detail}"))
        })
        .collect();
    let notes = out
        .stderr
        .lines()
        .filter(|line| line.starts_with("members:"))
        .map(str::to_owned)
        .collect();
    Seen {
        found,
        notes,
        stderr: out.stderr,
        code: out.code,
    }
}

fn scene(spec: &str, roots: &str, scope: &str, files: Files<'_>, env: Option<&Env<'_>>) -> Result<Seen, Fault> {
    let project = Project::with_rules(roots, &rule(spec, scope))?;
    env.map(|env| venv(&project, env)).transpose()?;
    for (path, text) in files {
        project.file(path, text)?;
    }
    Ok(seen(project.check(&["-v"])?))
}

fn within(spec: &str, files: Files<'_>) -> Result<Seen, Fault> {
    scene(spec, "[\"src\"]", "src/**/*.py", files, None)
}

fn single(spec: &str, code: &str) -> Result<Vec<String>, Fault> {
    Ok(within(spec, &[("src/pkg/m.py", code)])?.found)
}

fn on(spec: &str, files: Files<'_>, env: &Env<'_>) -> Result<Seen, Fault> {
    scene(spec, "[\"src\"]", "src/**/*.py", files, Some(env))
}

fn at(path: &str, line: u32, detail: &str) -> String {
    format!("{path}:{line} {detail}")
}

/// A verbose stderr without its `time: <phase> <ms>` lines; each of those must be well formed.
fn census_of(stderr: &str) -> Result<String, Fault> {
    let (times, census): (Vec<&str>, Vec<&str>) = stderr.lines().partition(|line| line.starts_with("time: "));
    for line in times {
        let (phase, millis) = line["time: ".len()..].split_once(' ').ok_or(line)?;
        assert!(!phase.is_empty() && millis.parse::<f64>().is_ok(), "{line}");
    }
    Ok(census.iter().flat_map(|line| [*line, "\n"]).collect())
}

fn here(line: u32, detail: &str) -> String {
    at("src/pkg/m.py", line, detail)
}

#[test]
fn members_orphan_private_serves_no_public() -> Outcome {
    let found = single(
        LINEAGE,
        "class A:\n    def _helper(self): ...\n    def run(self): ...\n",
    )?;
    assert_eq!(found, [here(2, "_helper serves no public of A")]);
    Ok(())
}

#[test]
fn members_finding_sits_at_the_methods_name_and_the_run_fails() -> Outcome {
    let project = Project::with_rules("[\"src\"]", &rule(LINEAGE, "src/**/*.py"))?;
    project.file("src/pkg/m.py", "class A:\n    async def _helper(self): ...\n")?;
    let out = project.check(&[])?;
    assert_eq!(
        out.stdout,
        "src/pkg/m.py:2:15: t.members genealogy: _helper serves no public of A\n\nFound 1 finding (1 error, 0 warnings).\n"
    );
    assert_eq!((out.stderr.as_str(), out.code), ("", 1));
    Ok(())
}

#[test]
fn members_private_names_the_public_it_serves_by_greedy_prefix() -> Outcome {
    let code = "class A:\n    def _run_fetch_sql(self): ...\n    def _run(self): ...\n    def _runner_x(self): ...\n    def run(self): ...\n";
    assert_eq!(
        single(LINEAGE, code)?,
        [
            here(3, "_run serves no public of A"),
            here(4, "_runner_x serves no public of A")
        ]
    );
    Ok(())
}

#[test]
fn members_private_serves_a_public_of_a_longer_name() -> Outcome {
    let code =
        "class A:\n    def _fetch_rows_sql(self): ...\n    def fetch_rows(self): ...\n    def fetch(self): ...\n";
    assert!(single(LINEAGE, code)?.is_empty());
    Ok(())
}

#[test]
fn members_common_stem_is_the_configured_word_only() -> Outcome {
    let code = "class A:\n    def _common_x(self): ...\n    def _shared_x(self): ...\n    def run(self): ...\n";
    assert_eq!(single(LINEAGE, code)?, [here(3, "_shared_x serves no public of A")]);
    assert_eq!(single(SHARED, code)?, [here(2, "_common_x serves no public of A")]);
    Ok(())
}

#[test]
fn members_common_stem_with_an_empty_list_admits_nothing() -> Outcome {
    let code = "class A:\n    def _common_x(self): ...\n";
    assert_eq!(
        single("lineage: {}", code)?,
        [here(2, "_common_x serves no public of A")]
    );
    Ok(())
}

#[test]
fn members_underscore_alone_and_a_stem_without_its_what_are_orphans() -> Outcome {
    let code = "class A:\n    def _(self): ...\n    def _run(self): ...\n    def run(self): ...\n";
    assert_eq!(
        single(LINEAGE, code)?,
        [
            here(2, "_ serves no public of A"),
            here(3, "_run serves no public of A")
        ]
    );
    Ok(())
}

#[test]
fn members_a_stem_closed_by_an_underscore_with_an_empty_what_is_an_orphan() -> Outcome {
    let code = "class A:\n    def run(self): ...\n    def _run_(self): ...\n    def _run_x(self): ...\n";
    assert_eq!(single(LINEAGE, code)?, [here(3, "_run_ serves no public of A")]);
    Ok(())
}

#[test]
fn members_a_sunder_is_exempt_unless_its_name_is_a_public() -> Outcome {
    let code = "class A:\n    def run(self): ...\n    def _missing_(cls, value): ...\n    def _walk_(self): ...\n\n\nclass B:\n    def walk(self): ...\n    def _walk_(self): ...\n    def _missing_(cls, value): ...\n";
    assert_eq!(single(LINEAGE, code)?, [here(9, "_walk_ serves no public of B")]);
    Ok(())
}

#[test]
fn members_dunder_stems_are_publics() -> Outcome {
    let code = "class A:\n    def _init_subclass_register(cls): ...\n    def _post_init_check(self): ...\n    def _call_x(self): ...\n    def _enter_x(self): ...\n    def _aenter_x(self): ...\n    def _aexit_x(self): ...\n    def _exit_x(self): ...\n    def _iter_x(self): ...\n    def __init_subclass__(cls): ...\n    def __post_init__(self): ...\n    def __call__(self): ...\n    async def __aenter__(self): ...\n    async def __aexit__(self, *rest): ...\n";
    assert_eq!(single(LINEAGE, code)?, [here(9, "_iter_x serves no public of A")]);
    Ok(())
}

#[test]
fn members_async_dunder_stem_serves_with_and_without_its_a() -> Outcome {
    let code =
        "class A:\n    def _enter_x(self): ...\n    def _aenter_x(self): ...\n    async def __aenter__(self): ...\n";
    assert!(single(LINEAGE, code)?.is_empty());
    Ok(())
}

#[test]
fn members_dunders_and_sunders_are_never_judged() -> Outcome {
    let code = "class A:\n    def __repr__(self): ...\n    def _missing_(cls, value): ...\n    def __x__(self): ...\n    def run(self): ...\n";
    assert!(single(BOTH, code)?.is_empty());
    Ok(())
}

#[test]
fn members_name_mangled_private_is_judged_as_a_private() -> Outcome {
    let code = "class A:\n    def __run_fetch(self): ...\n    def __orphan(self): ...\n    def run(self): ...\n";
    assert_eq!(single(LINEAGE, code)?, [here(3, "__orphan serves no public of A")]);
    Ok(())
}

#[test]
fn members_name_mangled_private_after_a_public_breaks_the_order() -> Outcome {
    let code = "class A:\n    def run(self): ...\n    def __run_fetch(self): ...\n";
    assert_eq!(single(ORDER, code)?, [here(3, "__run_fetch comes after public run")]);
    Ok(())
}

#[test]
fn members_decorated_and_async_methods_are_methods() -> Outcome {
    let code = "class A:\n    @property\n    def _size_raw(self): ...\n    @staticmethod\n    async def _common_clock(): ...\n    @classmethod\n    def _build_x(cls): ...\n    @functools.cache\n    def _stray(self): ...\n    @property\n    def size(self): ...\n    @classmethod\n    def build(cls): ...\n";
    assert_eq!(single(LINEAGE, code)?, [here(9, "_stray serves no public of A")]);
    Ok(())
}

#[test]
fn members_property_setter_is_one_method_judged_once() -> Outcome {
    let code =
        "class A:\n    @property\n    def _stray(self): ...\n    @_stray.setter\n    def _stray(self, value): ...\n";
    assert_eq!(single(LINEAGE, code)?, [here(3, "_stray serves no public of A")]);
    Ok(())
}

#[test]
fn members_property_setter_of_a_public_serves_its_privates() -> Outcome {
    let code = "class A:\n    def _size_check(self): ...\n    @property\n    def size(self): ...\n    @size.setter\n    def size(self, value): ...\n";
    assert!(single(LINEAGE, code)?.is_empty());
    Ok(())
}

#[test]
fn members_methods_inside_a_nested_statement_of_the_class_body_are_not_methods() -> Outcome {
    let code = "class A:\n    if True:\n        def _stray(self): ...\n";
    assert!(single(LINEAGE, code)?.is_empty());
    Ok(())
}

#[test]
fn members_class_with_no_methods_or_only_publics_has_nothing_to_judge() -> Outcome {
    let code = "class Empty:\n    size: int = 1\n\n\nclass Only:\n    def run(self): ...\n    def go(self): ...\n\n\nclass Bare: ...\n";
    assert!(single(BOTH, code)?.is_empty());
    Ok(())
}

#[test]
fn members_nested_class_is_judged_on_its_own_publics() -> Outcome {
    let code = "class Outer:\n    def run(self): ...\n\n    class Inner:\n        def _run_fetch(self): ...\n";
    assert_eq!(
        single(LINEAGE, code)?,
        [here(5, "_run_fetch serves no public of Inner")]
    );
    Ok(())
}

#[test]
fn members_class_in_a_function_and_in_a_nested_class_are_judged() -> Outcome {
    let code = "def make():\n    class Local:\n        def _stray(self): ...\n    return Local\n\n\nclass A:\n    class B:\n        class C:\n            def _stray(self): ...\n";
    assert_eq!(
        single(LINEAGE, code)?,
        [
            here(3, "_stray serves no public of Local"),
            here(10, "_stray serves no public of C")
        ]
    );
    Ok(())
}

#[test]
fn members_each_class_of_a_module_is_judged_in_its_own_right() -> Outcome {
    let code = "class A:\n    def run(self): ...\n\n\nclass B:\n    def _run_x(self): ...\n";
    assert_eq!(single(LINEAGE, code)?, [here(6, "_run_x serves no public of B")]);
    Ok(())
}

#[test]
fn members_a_method_a_hook_decorates_is_exempt_from_lineage_and_order() -> Outcome {
    let spec = format!("{BOTH}, hooks: [{MODEL}]");
    let code = "class A:\n    def run(self): ...\n    @field_validator(\"x\")\n    def _x_strip(cls, v): ...\n    @pydantic.model_validator(mode=\"after\")\n    def _check(self): ...\n    @other\n    def _stray(self): ...\n";
    assert_eq!(
        single(&spec, code)?,
        [
            here(8, "_stray comes after public run"),
            here(8, "_stray serves no public of A")
        ]
    );
    Ok(())
}

#[test]
fn members_without_hooks_the_validator_is_judged() -> Outcome {
    let code = "class A:\n    @field_validator(\"x\")\n    def _x_strip(cls, v): ...\n";
    assert_eq!(single(LINEAGE, code)?, [here(3, "_x_strip serves no public of A")]);
    Ok(())
}

#[test]
fn members_public_of_a_same_file_base_is_served() -> Outcome {
    let code = "class Root:\n    def run(self): ...\n\n\nclass Leaf(Root):\n    def _run_fetch(self): ...\n    def _stray_x(self): ...\n";
    assert_eq!(single(LINEAGE, code)?, [here(7, "_stray_x serves no public of Leaf")]);
    Ok(())
}

#[test]
fn members_same_file_base_defined_after_its_child_resolves() -> Outcome {
    let code = "class Leaf(Root):\n    def _run_fetch(self): ...\n    def _stray_x(self): ...\n\n\nclass Root:\n    def run(self): ...\n";
    assert_eq!(single(LINEAGE, code)?, [here(3, "_stray_x serves no public of Leaf")]);
    Ok(())
}

#[test]
fn members_publics_come_down_through_a_same_file_chain_and_both_bases() -> Outcome {
    let code = "class A:\n    def run(self): ...\n\n\nclass B(A): ...\n\n\nclass C:\n    def go(self): ...\n\n\nclass D(B, C):\n    def _run_x(self): ...\n    def _go_x(self): ...\n    def _gone_x(self): ...\n";
    assert_eq!(single(LINEAGE, code)?, [here(15, "_gone_x serves no public of D")]);
    Ok(())
}

#[test]
fn members_a_base_privates_serve_nothing_to_its_child() -> Outcome {
    let code = "class A:\n    def _run_x(self): ...\n\n\nclass B(A):\n    def _run_y(self): ...\n";
    assert_eq!(
        single(LINEAGE, code)?,
        [
            here(2, "_run_x serves no public of A"),
            here(6, "_run_y serves no public of B")
        ]
    );
    Ok(())
}

#[test]
fn members_a_subscripted_base_is_its_class() -> Outcome {
    let code = "class Root[T]:\n    def run(self): ...\n\n\nclass Leaf(Root[int]):\n    def _run_fetch(self): ...\n    def _stray_x(self): ...\n";
    assert_eq!(single(LINEAGE, code)?, [here(7, "_stray_x serves no public of Leaf")]);
    Ok(())
}

#[test]
fn members_a_keyword_in_the_bases_list_is_not_a_base() -> Outcome {
    let code = "class Leaf(Root, metaclass=Meta):\n    def _run_fetch(self): ...\n    def _stray_x(self): ...\n\n\nclass Root:\n    def run(self): ...\n\n\nclass Meta(type): ...\n";
    assert_eq!(single(LINEAGE, code)?, [here(3, "_stray_x serves no public of Leaf")]);
    Ok(())
}

#[test]
fn members_enum_subclass_private_serving_its_public_is_judged_clean() -> Outcome {
    let env = Env {
        stdlib: &[(
            "enum.py",
            "class Enum:\n    def __init__(self): ...\n\n\nclass StrEnum(str, Enum): ...\n",
        )],
        site: &[],
    };
    let code = "from enum import StrEnum\n\n\nclass Wear(StrEnum):\n    ONE = \"one\"\n\n    def _holds_search(self): ...\n    def _stray_x(self): ...\n    def holds(self): ...\n";
    let seen = on(LINEAGE, &[("src/pkg/m.py", code)], &env)?;
    assert_eq!(seen.found, [here(8, "_stray_x serves no public of Wear")]);
    Ok(())
}

const BASE: &str = "class Base:\n    def run(self): ...\n";

#[test]
fn members_imported_project_base_by_from_import() -> Outcome {
    let child = "from pkg.base import Base\n\n\nclass Leaf(Base):\n    def _run_fetch(self): ...\n    def _stray_x(self): ...\n";
    let seen = within(LINEAGE, &[("src/pkg/base.py", BASE), ("src/pkg/child.py", child)])?;
    assert_eq!(
        seen.found,
        [at("src/pkg/child.py", 6, "_stray_x serves no public of Leaf")]
    );
    Ok(())
}

#[test]
fn members_imported_project_base_by_every_absolute_spelling() -> Outcome {
    for (import, base) in [
        ("import pkg.base", "pkg.base.Base"),
        ("from pkg import base", "base.Base"),
        ("from pkg.base import Base as B", "B"),
        ("import pkg.base as pb", "pb.Base"),
        ("from pkg.base import (\n    Base,\n)", "Base"),
    ] {
        let child =
            format!("{import}\n\n\nclass Leaf({base}):\n    def _run_fetch(self): ...\n    def _stray_x(self): ...\n");
        let seen = within(LINEAGE, &[("src/pkg/base.py", BASE), ("src/pkg/child.py", &child)])?;
        assert_eq!(seen.found.len(), 1, "{import}: {:?}", seen.found);
        assert!(seen.found[0].ends_with("_stray_x serves no public of Leaf"), "{import}");
        assert_eq!(seen.found[0].matches("child.py").count(), 1, "{import}");
    }
    Ok(())
}

#[test]
fn members_project_base_is_found_in_a_package_and_through_its_init_reexport() -> Outcome {
    let init = "from pkg.base import Base\n";
    let child =
        "from pkg import Base\n\n\nclass Leaf(Base):\n    def _run_fetch(self): ...\n    def _stray_x(self): ...\n";
    let seen = within(
        LINEAGE,
        &[
            ("src/pkg/__init__.py", init),
            ("src/pkg/base.py", BASE),
            ("src/other/child.py", child),
        ],
    )?;
    assert_eq!(
        seen.found,
        [at("src/other/child.py", 6, "_stray_x serves no public of Leaf")]
    );
    Ok(())
}

#[test]
fn members_project_base_in_a_package_init_file_itself() -> Outcome {
    let child =
        "from pkg import Base\n\n\nclass Leaf(Base):\n    def _run_fetch(self): ...\n    def _stray_x(self): ...\n";
    let seen = within(LINEAGE, &[("src/pkg/__init__.py", BASE), ("src/other/child.py", child)])?;
    assert_eq!(
        seen.found,
        [at("src/other/child.py", 6, "_stray_x serves no public of Leaf")]
    );
    Ok(())
}

#[test]
fn members_project_bases_resolve_transitively_across_three_modules() -> Outcome {
    let middle = "from pkg.base import Base\n\n\nclass Middle(Base):\n    def go(self): ...\n";
    let leaf = "from pkg.middle import Middle\n\n\nclass Leaf(Middle):\n    def _run_a(self): ...\n    def _go_b(self): ...\n    def _gone_c(self): ...\n";
    let seen = within(
        LINEAGE,
        &[
            ("src/pkg/base.py", BASE),
            ("src/pkg/middle.py", middle),
            ("src/pkg/leaf.py", leaf),
        ],
    )?;
    assert_eq!(
        seen.found,
        [at("src/pkg/leaf.py", 7, "_gone_c serves no public of Leaf")]
    );
    Ok(())
}

#[test]
fn members_each_source_root_is_searched_in_order() -> Outcome {
    let child =
        "from base import Base\n\n\nclass Leaf(Base):\n    def _run_fetch(self): ...\n    def _stray_x(self): ...\n";
    let seen = scene(
        LINEAGE,
        "[\"src\", \"lib\"]",
        "src/**/*.py",
        &[("lib/base.py", BASE), ("src/child.py", child)],
        None,
    )?;
    assert_eq!(seen.found, [at("src/child.py", 6, "_stray_x serves no public of Leaf")]);
    Ok(())
}

#[test]
fn members_a_root_given_with_dot_slash_resolves_like_the_bare_one() -> Outcome {
    let child = "from pkg.base import Base\n\n\nclass Leaf(Base):\n    def _run_fetch(self): ...\n    def _stray_x(self): ...\n";
    let seen = scene(
        LINEAGE,
        "[\"./src\"]",
        "src/**/*.py",
        &[("src/pkg/base.py", BASE), ("src/pkg/child.py", child)],
        None,
    )?;
    assert_eq!(
        seen.found,
        [at("src/pkg/child.py", 6, "_stray_x serves no public of Leaf")]
    );
    Ok(())
}

#[test]
fn members_unresolved_base_judges_neither_lineage_nor_order() -> Outcome {
    let code = "from requests import Session\n\n\nclass Client(Session):\n    def run(self): ...\n    def _orphan(self): ...\n";
    let seen = within(BOTH, &[("src/pkg/m.py", code)])?;
    assert!(seen.found.is_empty(), "{:?}", seen.found);
    assert_eq!(
        seen.notes,
        [
            "members: environment: no .venv, no foreign roots",
            "members: 0 classes judged, 1 skipped (unresolved base), 0 on a builtin base; modules read: 0 project, 0 foreign",
            "members: unresolved: requests.Session \u{d7}1 (no such module)",
        ]
    );
    Ok(())
}

#[test]
fn members_unresolved_base_with_lineage_alone_is_silent() -> Outcome {
    let code = "from requests import Session\n\n\nclass Client(Session):\n    def _orphan(self): ...\n";
    let seen = within(LINEAGE, &[("src/pkg/m.py", code)])?;
    assert!(seen.found.is_empty() && seen.code == 0, "{:?}", seen.found);
    Ok(())
}

#[test]
fn members_one_unresolved_base_among_resolved_ones_skips_the_class() -> Outcome {
    let code = "from requests import Session\n\n\nclass Own:\n    def run(self): ...\n\n\nclass Both(Own, Session):\n    def _stray(self): ...\n";
    let seen = within(LINEAGE, &[("src/pkg/m.py", code)])?;
    assert!(seen.found.is_empty(), "{:?}", seen.found);
    Ok(())
}

#[test]
fn members_a_class_below_an_unresolved_class_is_skipped_too() -> Outcome {
    let code =
        "from requests import Session\n\n\nclass Mid(Session): ...\n\n\nclass Leaf(Mid):\n    def _stray(self): ...\n";
    let seen = within(LINEAGE, &[("src/pkg/m.py", code)])?;
    assert!(seen.found.is_empty(), "{:?}", seen.found);
    assert!(
        seen.notes
            .contains(&"members: unresolved: requests.Session \u{d7}2 (no such module)".to_owned()),
        "{:?}",
        seen.notes
    );
    assert!(
        !seen.notes.iter().any(|note| note.contains("unresolved: Mid")),
        "the root cause is named, not the class below it: {:?}",
        seen.notes
    );
    Ok(())
}

#[test]
fn members_each_reason_for_an_unresolved_base_is_named() -> Outcome {
    let site: Files<'_> = &[
        ("lazy/__init__.py", "def __getattr__(name): ...\n"),
        ("star/__init__.py", "from star.gone import *\n"),
        ("dyn/__init__.py", "C = make()\n"),
        ("bare/__init__.py", "x = 1\n"),
        ("comp.cpython-314-x86_64-linux-gnu.so", ""),
        ("cyc/__init__.py", "from cyc import Loop\n"),
    ];
    let env = Env { stdlib: &[], site };
    let code = "from lazy import A\nfrom star import B\nfrom dyn import C\nfrom bare import D\nfrom comp import E\nfrom nowhere import F\nfrom cyc import Loop\nfrom pkg import G\n\n\nclass Ca(A): ...\n\n\nclass Cb(B): ...\n\n\nclass Cc(C): ...\n\n\nclass Cd(D): ...\n\n\nclass Ce(E): ...\n\n\nclass Cf(F): ...\n\n\nclass Cg(Loop): ...\n\n\nclass Ch(G): ...\n\n\nclass Ci(make()): ...\n";
    let seen = on(
        LINEAGE,
        &[("src/pkg/m.py", code), ("src/pkg/__init__.py", "from .m import G\n")],
        &env,
    )?;
    let mut notes: Vec<&str> = seen.notes.iter().skip(2).map(String::as_str).collect();
    notes.sort_unstable();
    assert_eq!(
        notes,
        [
            "members: unresolved: bare.D \u{d7}1 (no such name)",
            "members: unresolved: comp.E \u{d7}1 (compiled module)",
            "members: unresolved: cyc.Loop \u{d7}1 (cycle)",
            "members: unresolved: dyn.C \u{d7}1 (dynamic base)",
            "members: unresolved: lazy.A \u{d7}1 (lazy export)",
            "members: unresolved: make() \u{d7}1 (dynamic base)",
            "members: unresolved: nowhere.F \u{d7}1 (no such module)",
            "members: unresolved: pkg.G \u{d7}1 (relative import)",
            "members: unresolved: star.B \u{d7}1 (star import)",
        ]
    );
    Ok(())
}

#[test]
fn members_project_relative_import_is_unresolved_never_guessed() -> Outcome {
    let child = "from .base import Base\n\n\nclass Leaf(Base):\n    def _stray(self): ...\n";
    let seen = within(LINEAGE, &[("src/pkg/base.py", BASE), ("src/pkg/child.py", child)])?;
    assert!(seen.found.is_empty(), "{:?}", seen.found);
    assert!(
        seen.notes
            .contains(&"members: unresolved: .base.Base \u{d7}1 (relative import)".to_owned()),
        "{:?}",
        seen.notes
    );
    Ok(())
}

#[test]
fn members_project_relative_import_of_the_package_is_unresolved_too() -> Outcome {
    let child = "from . import base\n\n\nclass Leaf(base.Base):\n    def _stray(self): ...\n";
    let seen = within(LINEAGE, &[("src/pkg/base.py", BASE), ("src/pkg/child.py", child)])?;
    assert!(seen.found.is_empty(), "{:?}", seen.found);
    Ok(())
}

#[test]
fn members_an_import_that_names_nothing_the_module_binds_is_no_such_name() -> Outcome {
    let child = "from pkg.base import Missing\n\n\nclass Leaf(Missing):\n    def _stray(self): ...\n";
    let seen = within(LINEAGE, &[("src/pkg/base.py", BASE), ("src/pkg/child.py", child)])?;
    assert!(seen.found.is_empty(), "{:?}", seen.found);
    assert!(
        seen.notes
            .contains(&"members: unresolved: pkg.base.Missing \u{d7}1 (no such name)".to_owned()),
        "{:?}",
        seen.notes
    );
    Ok(())
}

#[test]
fn members_a_base_bound_to_a_value_or_an_expression_is_a_dynamic_base() -> Outcome {
    let code = "Base = make()\n\n\nclass A(Base):\n    def _stray(self): ...\n\n\nclass B(make_base()):\n    def _stray(self): ...\n\n\nclass C(Registry.get(\"x\")):\n    def _stray(self): ...\n";
    let seen = within(LINEAGE, &[("src/pkg/m.py", code)])?;
    assert!(seen.found.is_empty(), "{:?}", seen.found);
    assert!(
        seen.notes
            .iter()
            .all(|note| !note.contains("builtin base") || note.contains("0 classes judged"))
    );
    assert_eq!(
        seen.notes.iter().filter(|note| note.contains("dynamic base")).count(),
        3,
        "{:?}",
        seen.notes
    );
    Ok(())
}

#[test]
fn members_inheritance_cycle_terminates_and_skips_its_classes() -> Outcome {
    let code =
        "class A(B):\n    def _stray(self): ...\n\n\nclass B(A):\n    def _stray(self): ...\n\n\nclass C(C): ...\n";
    let seen = within(LINEAGE, &[("src/pkg/m.py", code)])?;
    assert!(seen.found.is_empty(), "{:?}", seen.found);
    assert!(
        seen.notes.iter().any(|note| note.ends_with("(cycle)")),
        "{:?}",
        seen.notes
    );
    Ok(())
}

#[test]
fn members_cycle_across_project_modules_terminates() -> Outcome {
    let a = "from pkg.b import B\n\n\nclass A(B):\n    def _stray(self): ...\n";
    let b = "from pkg.a import A\n\n\nclass B(A):\n    def _stray(self): ...\n";
    let seen = within(LINEAGE, &[("src/pkg/a.py", a), ("src/pkg/b.py", b)])?;
    assert!(seen.found.is_empty(), "{:?}", seen.found);
    assert!(
        seen.notes.iter().any(|note| note.ends_with("(cycle)")),
        "{:?}",
        seen.notes
    );
    Ok(())
}

#[test]
fn members_cycle_judges_neither_lineage_nor_order() -> Outcome {
    let code = "class A(B):\n    def run(self): ...\n    def _late(self): ...\n\n\nclass B(A): ...\n";
    let seen = within(BOTH, &[("src/pkg/m.py", code)])?;
    assert!(seen.found.is_empty(), "{:?}", seen.found);
    assert!(
        seen.notes.iter().any(|note| note.contains("2 skipped")),
        "{:?}",
        seen.notes
    );
    Ok(())
}

#[test]
fn members_a_bare_builtin_base_is_counted_and_contributes_no_publics() -> Outcome {
    let code = "class Bag(dict):\n    def _fill_x(self): ...\n    def fill(self): ...\n\n\nclass Plain(object):\n    def _stray(self): ...\n\n\nclass Implicit:\n    def _stray(self): ...\n";
    let seen = within(LINEAGE, &[("src/pkg/m.py", code)])?;
    assert_eq!(
        seen.found,
        [
            here(7, "_stray serves no public of Plain"),
            here(11, "_stray serves no public of Implicit")
        ]
    );
    assert_eq!(
        seen.notes[1],
        "members: 3 classes judged, 0 skipped (unresolved base), 1 on a builtin base; modules read: 0 project, 0 foreign"
    );
    Ok(())
}

#[test]
fn members_object_alone_is_not_a_builtin_base() -> Outcome {
    let code = "class A(object): ...\n\n\nclass B(A): ...\n";
    let seen = within(LINEAGE, &[("src/pkg/m.py", code)])?;
    assert_eq!(
        seen.notes[1],
        "members: 2 classes judged, 0 skipped (unresolved base), 0 on a builtin base; modules read: 0 project, 0 foreign"
    );
    Ok(())
}

#[test]
fn members_builtin_base_is_inherited_down_the_line() -> Outcome {
    let code = "class Bag(dict): ...\n\n\nclass Leaf(Bag): ...\n\n\nclass Mixed(Leaf, object): ...\n";
    let seen = within(LINEAGE, &[("src/pkg/m.py", code)])?;
    assert_eq!(
        seen.notes[1],
        "members: 3 classes judged, 0 skipped (unresolved base), 3 on a builtin base; modules read: 0 project, 0 foreign"
    );
    Ok(())
}

#[test]
fn members_a_builtin_subclass_private_serving_a_builtin_public_is_served_and_a_stray_one_is_an_orphan() -> Outcome {
    let code = "class Bag(dict):\n    def _get_x(self): ...\n    def _update_x(self): ...\n    def _stray(self): ...\n";
    assert_eq!(single(LINEAGE, code)?, [here(4, "_stray serves no public of Bag")]);
    Ok(())
}

fn override_in(base_root: &str, scope: &str) -> Result<Seen, Fault> {
    let base = "class Base:\n    def _hook_x(self): ...\n    def run(self): ...\n";
    let leaf =
        "from base import Base\n\n\nclass Leaf(Base):\n    def _hook_x(self): ...\n    def _other_y(self): ...\n";
    scene(
        LINEAGE,
        &format!("[\"src\", \"{base_root}\"]"),
        scope,
        &[(&format!("{base_root}/base.py"), base), ("src/leaf.py", leaf)],
        None,
    )
}

#[test]
fn members_override_of_an_ancestor_outside_the_rules_scope_is_exempt() -> Outcome {
    let seen = override_in("vendor", "src/**/*.py")?;
    assert_eq!(seen.found, [at("src/leaf.py", 6, "_other_y serves no public of Leaf")]);
    Ok(())
}

#[test]
fn members_override_of_an_ancestor_inside_the_rules_scope_is_not_exempt() -> Outcome {
    let seen = override_in("lib", "{src,lib}/**/*.py")?;
    assert_eq!(
        seen.found,
        [
            at("lib/base.py", 2, "_hook_x serves no public of Base"),
            at("src/leaf.py", 5, "_hook_x serves no public of Leaf"),
            at("src/leaf.py", 6, "_other_y serves no public of Leaf")
        ]
    );
    Ok(())
}

#[test]
fn members_override_of_a_foreign_ancestor_is_exempt_and_other_privates_are_not() -> Outcome {
    let env = Env {
        stdlib: &[],
        site: &[(
            "lib1/__init__.py",
            "class Base:\n    def _hook_x(self): ...\n    def run(self): ...\n",
        )],
    };
    let code = "from lib1 import Base\n\n\nclass Leaf(Base):\n    def _hook_x(self): ...\n    def _run_y(self): ...\n    def _other_y(self): ...\n";
    let seen = on(LINEAGE, &[("src/pkg/m.py", code)], &env)?;
    assert_eq!(seen.found, [here(7, "_other_y serves no public of Leaf")]);
    Ok(())
}

#[test]
fn members_foreign_override_is_exempt_from_the_order_too() -> Outcome {
    let env = Env {
        stdlib: &[],
        site: &[("lib1/__init__.py", "class Base:\n    def _hook_x(self): ...\n")],
    };
    let code = "from lib1 import Base\n\n\nclass Leaf(Base):\n    def run(self): ...\n    def _hook_x(self): ...\n    def _late_y(self): ...\n";
    let seen = on(ORDER, &[("src/pkg/m.py", code)], &env)?;
    assert_eq!(seen.found, [here(7, "_late_y comes after public run")]);
    Ok(())
}

#[test]
fn members_override_by_a_non_override_name_of_the_same_stem_is_not_exempt() -> Outcome {
    let env = Env {
        stdlib: &[],
        site: &[("lib1/__init__.py", "class Base:\n    def _hook_x(self): ...\n")],
    };
    let code = "from lib1 import Base\n\n\nclass Leaf(Base):\n    def _hook_xx(self): ...\n";
    let seen = on(LINEAGE, &[("src/pkg/m.py", code)], &env)?;
    assert_eq!(seen.found, [here(5, "_hook_xx serves no public of Leaf")]);
    Ok(())
}

const FOREIGN: &str = "class Thing:\n    def run(self): ...\n";

fn leaf(import: &str, base: &str) -> String {
    format!("{import}\n\n\nclass Leaf({base}):\n    def _run_fetch(self): ...\n    def _stray_x(self): ...\n")
}

fn stray(seen: &Seen) -> bool {
    seen.found == [here(6, "_stray_x serves no public of Leaf")]
}

fn foreign(site: Files<'_>, import: &str, base: &str) -> Result<Seen, Fault> {
    let env = Env { stdlib: &[], site };
    on(LINEAGE, &[("src/pkg/m.py", &leaf(import, base))], &env)
}

#[test]
fn members_foreign_base_in_a_plain_module() -> Outcome {
    let seen = foreign(&[("lib1.py", FOREIGN)], "from lib1 import Thing", "Thing")?;
    assert!(stray(&seen), "{:?}", seen.found);
    assert_eq!(
        seen.notes[1],
        "members: 1 classes judged, 0 skipped (unresolved base), 0 on a builtin base; modules read: 0 project, 1 foreign"
    );
    Ok(())
}

#[test]
fn members_foreign_base_in_a_package_init_and_in_a_submodule() -> Outcome {
    for (site, import) in [
        (vec![("lib1/__init__.py", FOREIGN)], "from lib1 import Thing"),
        (
            vec![("lib1/__init__.py", ""), ("lib1/core.py", FOREIGN)],
            "from lib1.core import Thing",
        ),
        (
            vec![("lib1/__init__.py", ""), ("lib1/core.py", FOREIGN)],
            "import lib1.core",
        ),
    ] {
        let base = if import.starts_with("import") {
            "lib1.core.Thing"
        } else {
            "Thing"
        };
        let seen = foreign(&site, import, base)?;
        assert!(stray(&seen), "{import}: {:?}", seen.found);
    }
    Ok(())
}

#[test]
fn members_foreign_base_through_an_init_reexport() -> Outcome {
    let seen = foreign(
        &[
            ("lib1/__init__.py", "from lib1.core import Thing\n"),
            ("lib1/core.py", FOREIGN),
        ],
        "from lib1 import Thing",
        "Thing",
    )?;
    assert!(stray(&seen), "{:?}", seen.found);
    Ok(())
}

#[test]
fn members_foreign_base_through_relative_imports_inside_the_package() -> Outcome {
    for (name, site) in [
        (
            "from .x import Y",
            vec![("lib1/__init__.py", "from .x import Thing\n"), ("lib1/x.py", FOREIGN)],
        ),
        (
            "from ..x import Y",
            vec![
                ("lib1/__init__.py", "from lib1.sub import Thing\n"),
                ("lib1/sub/__init__.py", "from ..x import Thing\n"),
                ("lib1/x.py", FOREIGN),
            ],
        ),
        (
            "from . import x",
            vec![
                ("lib1/__init__.py", "from . import x\n\n\nclass Thing(x.Core): ...\n"),
                ("lib1/x.py", "class Core:\n    def run(self): ...\n"),
            ],
        ),
        (
            "from .sub.deep import Y",
            vec![
                ("lib1/__init__.py", "from .sub.deep import Thing\n"),
                ("lib1/sub/__init__.py", ""),
                ("lib1/sub/deep.py", FOREIGN),
            ],
        ),
        (
            "relative import in a module file",
            vec![
                ("lib1/__init__.py", "from lib1.mid import Thing\n"),
                ("lib1/mid.py", "from .base import Thing\n"),
                ("lib1/base.py", FOREIGN),
            ],
        ),
    ] {
        let seen = foreign(&site, "from lib1 import Thing", "Thing")?;
        assert!(stray(&seen), "{name}: {:?} {:?}", seen.found, seen.notes);
    }
    Ok(())
}

#[test]
fn members_foreign_relative_import_climbing_past_the_root_is_unresolved() -> Outcome {
    let seen = foreign(
        &[("lib1/__init__.py", "from ... import Thing\n")],
        "from lib1 import Thing",
        "Thing",
    )?;
    assert!(seen.found.is_empty(), "{:?}", seen.found);
    assert!(
        seen.notes
            .contains(&"members: unresolved: lib1.Thing \u{d7}1 (no such module)".to_owned()),
        "{:?}",
        seen.notes
    );
    Ok(())
}

#[test]
fn members_foreign_base_in_a_stub_only_package() -> Outcome {
    let seen = foreign(&[("lib1/__init__.pyi", FOREIGN)], "from lib1 import Thing", "Thing")?;
    assert!(stray(&seen), "{:?}", seen.found);
    Ok(())
}

#[test]
fn members_foreign_stub_beside_a_source_is_the_fallback_for_a_name_the_source_does_not_bind() -> Outcome {
    let seen = foreign(
        &[
            ("lib1/__init__.py", "from lib1._core import Thing\n"),
            ("lib1/__init__.pyi", FOREIGN),
            ("lib1/_core.cpython-314-x86_64-linux-gnu.so", ""),
        ],
        "from lib1 import Thing",
        "Thing",
    )?;
    assert!(stray(&seen), "{:?}", seen.found);
    Ok(())
}

#[test]
fn members_foreign_stub_fallback_for_a_name_the_source_leaves_out() -> Outcome {
    let seen = foreign(
        &[("lib1/__init__.py", "x = 1\n"), ("lib1/__init__.pyi", FOREIGN)],
        "from lib1 import Thing",
        "Thing",
    )?;
    assert!(stray(&seen), "{:?}", seen.found);
    Ok(())
}

#[test]
fn members_foreign_source_wins_over_its_stub_when_it_binds_the_name() -> Outcome {
    let seen = foreign(
        &[
            ("lib1/__init__.py", "class Thing:\n    def run(self): ...\n"),
            ("lib1/__init__.pyi", "class Thing:\n    def other(self): ...\n"),
        ],
        "from lib1 import Thing",
        "Thing",
    )?;
    assert!(stray(&seen), "{:?}", seen.found);
    Ok(())
}

#[test]
fn members_compiled_module_with_no_stub_is_unresolved() -> Outcome {
    let seen = foreign(
        &[("lib1.cpython-314-x86_64-linux-gnu.so", "")],
        "from lib1 import Thing",
        "Thing",
    )?;
    assert!(seen.found.is_empty(), "{:?}", seen.found);
    assert!(
        seen.notes
            .contains(&"members: unresolved: lib1.Thing \u{d7}1 (compiled module)".to_owned()),
        "{:?}",
        seen.notes
    );
    Ok(())
}

#[test]
fn members_lazy_module_getattr_export_is_unresolved() -> Outcome {
    let seen = foreign(
        &[
            ("lib1/__init__.py", "def __getattr__(name): ...\n"),
            ("lib1/core.py", FOREIGN),
        ],
        "from lib1 import Thing",
        "Thing",
    )?;
    assert!(seen.found.is_empty(), "{:?}", seen.found);
    assert!(
        seen.notes
            .contains(&"members: unresolved: lib1.Thing \u{d7}1 (lazy export)".to_owned()),
        "{:?}",
        seen.notes
    );
    Ok(())
}

#[test]
fn members_lazy_module_that_writes_the_class_itself_resolves() -> Outcome {
    let seen = foreign(
        &[(
            "lib1/__init__.py",
            "def __getattr__(name): ...\n\n\nclass Thing:\n    def run(self): ...\n",
        )],
        "from lib1 import Thing",
        "Thing",
    )?;
    assert!(stray(&seen), "{:?}", seen.found);
    Ok(())
}

#[test]
fn members_star_import_is_followed_to_the_class() -> Outcome {
    let seen = foreign(
        &[
            ("lib1/__init__.py", "from lib1.core import *\n"),
            ("lib1/core.py", FOREIGN),
        ],
        "from lib1 import Thing",
        "Thing",
    )?;
    assert!(stray(&seen), "{:?}", seen.found);
    Ok(())
}

#[test]
fn members_star_import_is_followed_through_a_relative_star() -> Outcome {
    let seen = foreign(
        &[("lib1/__init__.py", "from .core import *\n"), ("lib1/core.py", FOREIGN)],
        "from lib1 import Thing",
        "Thing",
    )?;
    assert!(stray(&seen), "{:?}", seen.found);
    Ok(())
}

#[test]
fn members_star_import_that_fails_leaves_the_base_unresolved() -> Outcome {
    let seen = foreign(
        &[("lib1/__init__.py", "from lib1.gone import *\n")],
        "from lib1 import Thing",
        "Thing",
    )?;
    assert!(seen.found.is_empty(), "{:?}", seen.found);
    assert!(
        seen.notes
            .contains(&"members: unresolved: lib1.Thing \u{d7}1 (star import)".to_owned()),
        "{:?}",
        seen.notes
    );
    Ok(())
}

#[test]
fn members_star_import_that_resolves_but_lacks_the_name_still_fails() -> Outcome {
    let seen = foreign(
        &[
            ("lib1/__init__.py", "from lib1.core import *\n"),
            ("lib1/core.py", "class Other: ...\n"),
        ],
        "from lib1 import Thing",
        "Thing",
    )?;
    assert!(seen.found.is_empty(), "{:?}", seen.found);
    assert!(
        seen.notes
            .iter()
            .any(|note| note.contains("lib1.Thing") && note.contains("star import")),
        "{:?}",
        seen.notes
    );
    Ok(())
}

#[test]
fn members_first_binding_of_a_name_wins_in_a_try_import() -> Outcome {
    let init = "try:\n    from lib1.fast import Thing\nexcept ImportError:\n    from lib1.slow import Thing\n";
    let site: Files<'_> = &[
        ("lib1/__init__.py", init),
        ("lib1/fast.py", "class Thing:\n    def run(self): ...\n"),
        ("lib1/slow.py", "class Thing:\n    def walk(self): ...\n"),
    ];
    let seen = foreign(site, "from lib1 import Thing", "Thing")?;
    assert!(stray(&seen), "{:?}", seen.found);
    Ok(())
}

#[test]
fn members_first_binding_of_a_name_wins_under_type_checking() -> Outcome {
    let init = "from typing import TYPE_CHECKING\n\nif TYPE_CHECKING:\n    from lib1.typed import Thing\nelse:\n    from lib1.real import Thing\n";
    let site: Files<'_> = &[
        ("lib1/__init__.py", init),
        ("lib1/typed.py", "class Thing:\n    def run(self): ...\n"),
        ("lib1/real.py", "class Thing:\n    def walk(self): ...\n"),
    ];
    let seen = foreign(site, "from lib1 import Thing", "Thing")?;
    assert!(stray(&seen), "{:?}", seen.found);
    Ok(())
}

#[test]
fn members_first_binding_in_the_project_module_is_the_one_a_class_resolves_through() -> Outcome {
    let code = "try:\n    from lib1.fast import Thing\nexcept ImportError:\n    from lib1.slow import Thing\n\n\nclass Leaf(Thing):\n    def _run_fetch(self): ...\n    def _walk_x(self): ...\n";
    let env = Env {
        stdlib: &[],
        site: &[
            ("lib1/fast.py", "class Thing:\n    def run(self): ...\n"),
            ("lib1/slow.py", "class Thing:\n    def walk(self): ...\n"),
        ],
    };
    let seen = on(LINEAGE, &[("src/pkg/m.py", code)], &env)?;
    assert_eq!(seen.found, [here(9, "_walk_x serves no public of Leaf")]);
    Ok(())
}

#[test]
fn members_an_import_inside_a_function_binds_no_module_name() -> Outcome {
    let code = "def f():\n    from lib1 import Thing\n\n\nclass Leaf(Thing):\n    def _stray(self): ...\n";
    let env = Env {
        stdlib: &[],
        site: &[("lib1/__init__.py", FOREIGN)],
    };
    let seen = on(LINEAGE, &[("src/pkg/m.py", code)], &env)?;
    assert!(seen.found.is_empty(), "{:?}", seen.found);
    assert!(
        seen.notes[1].contains("0 classes judged") && seen.notes[1].contains("1 skipped"),
        "{:?}",
        seen.notes
    );
    assert_eq!(seen.notes[2], "members: unresolved: Thing \u{d7}1 (no such name)");
    Ok(())
}

#[test]
fn members_stdlib_is_searched_before_site_packages_and_the_project_before_both() -> Outcome {
    let env = Env {
        stdlib: &[("shadow.py", "class Base:\n    def go(self): ...\n")],
        site: &[("shadow.py", "class Base:\n    def run(self): ...\n")],
    };
    let code = "from shadow import Base\n\n\nclass Leaf(Base):\n    def _go_x(self): ...\n    def _run_x(self): ...\n";
    let seen = on(LINEAGE, &[("src/pkg/m.py", code)], &env)?;
    assert_eq!(seen.found, [here(6, "_run_x serves no public of Leaf")]);
    let own = on(
        LINEAGE,
        &[
            ("src/pkg/m.py", code),
            ("src/shadow.py", "class Base:\n    def walk(self): ...\n"),
        ],
        &env,
    )?;
    assert_eq!(
        own.found,
        [
            here(5, "_go_x serves no public of Leaf"),
            here(6, "_run_x serves no public of Leaf")
        ]
    );
    Ok(())
}

#[test]
fn members_stdlib_compiled_dynload_module_is_searched() -> Outcome {
    let env = Env {
        stdlib: &[("lib-dynload/_speedy.cpython-314-x86_64-linux-gnu.so", "")],
        site: &[],
    };
    let code = "from _speedy import Thing\n\n\nclass Leaf(Thing):\n    def _stray(self): ...\n";
    let seen = on(LINEAGE, &[("src/pkg/m.py", code)], &env)?;
    assert!(seen.found.is_empty(), "{:?}", seen.found);
    assert!(
        seen.notes.iter().any(|note| note.ends_with("(compiled module)")),
        "{:?}",
        seen.notes
    );
    Ok(())
}

#[test]
fn members_stdlib_import_from_a_module_no_root_holds_leaves_the_class_unresolved() -> Outcome {
    let env = Env {
        stdlib: &[("mylib.py", "from _gone import Core\n\n\nclass Wrapper(Core): ...\n")],
        site: &[("other/__init__.py", "from _gone import Core\n")],
    };
    let code = "from mylib import Wrapper\nfrom other import Core\n\n\nclass Leaf(Wrapper):\n    def _stray(self): ...\n\n\nclass Site(Core):\n    def _stray(self): ...\n";
    let seen = on(LINEAGE, &[("src/pkg/m.py", code)], &env)?;
    assert!(seen.found.is_empty(), "{:?}", seen.found);
    assert_eq!(
        seen.notes[1],
        "members: 0 classes judged, 2 skipped (unresolved base), 0 on a builtin base; modules read: 0 project, 2 foreign"
    );
    assert_eq!(
        seen.notes[2..],
        [
            "members: unresolved: _gone.Core \u{d7}1 (no such module)",
            "members: unresolved: other.Core \u{d7}1 (no such module)"
        ]
    );
    Ok(())
}

#[test]
fn members_stdlib_class_with_an_stdlib_base_is_walked_for_its_publics() -> Outcome {
    let env = Env {
        stdlib: &[("abc.py", "class ABC:\n    def register(self): ...\n")],
        site: &[],
    };
    let code =
        "from abc import ABC\n\n\nclass Leaf(ABC):\n    def _register_x(self): ...\n    def _stray_y(self): ...\n";
    let seen = on(LINEAGE, &[("src/pkg/m.py", code)], &env)?;
    assert_eq!(seen.found, [here(6, "_stray_y serves no public of Leaf")]);
    Ok(())
}

#[test]
fn members_without_verbose_says_nothing_on_stderr() -> Outcome {
    let project = Project::with_rules("[\"src\"]", &rule(LINEAGE, "src/**/*.py"))?;
    project.file("src/pkg/m.py", "class A: ...\n")?;
    let out = project.check(&[])?;
    assert_eq!((out.stderr.as_str(), out.code), ("", 0));
    Ok(())
}

#[test]
fn members_verbose_flag_is_global_before_or_after_the_command() -> Outcome {
    let project = Project::with_rules("[\"src\"]", &rule(LINEAGE, "src/**/*.py"))?;
    project.file("src/pkg/m.py", "class A: ...\n")?;
    let after = project.run(&["check", "-v"])?;
    let before = project.run(&["--verbose", "check"])?;
    let (after_census, before_census) = (census_of(&after.stderr)?, census_of(&before.stderr)?);
    assert_eq!(after_census, before_census);
    assert!(after_census.starts_with("members: environment:"), "{}", after.stderr);
    assert_eq!(after.stdout, "All checks passed.\n");
    Ok(())
}

#[test]
fn members_verbose_census_of_a_whole_scene_is_exact() -> Outcome {
    let env = Env {
        stdlib: &[("enum.py", "class Enum: ...\n")],
        site: &[("lib1/__init__.py", FOREIGN)],
    };
    let a = "from pkg.base import Base\nfrom lib1 import Thing\nfrom requests import Session\nfrom enum import Enum\n\n\nclass A(Base): ...\n\n\nclass B(Thing): ...\n\n\nclass C(Session): ...\n\n\nclass D(Session): ...\n\n\nclass E(Enum): ...\n\n\nclass F(str): ...\n";
    let b = "from pkg.base import Base\n\n\nclass G(Base): ...\n\n\nclass H(nope.Thing): ...\n";
    let project = Project::with_rules("[\"src\"]", &rule(LINEAGE, "src/**/*.py"))?;
    venv(&project, &env)?;
    for (path, text) in [("src/pkg/base.py", BASE), ("src/pkg/a.py", a), ("src/pkg/b.py", b)] {
        project.file(path, text)?;
    }
    let out = project.check(&["-v"])?;
    let root = project.path().canonicalize()?;
    let stdlib = root.join("py/lib/python3.14");
    assert_eq!(
        census_of(&out.stderr)?,
        format!(
            "members: environment: {}: stdlib {}; 2 foreign roots\nmembers: 6 classes judged, 3 skipped (unresolved base), 1 on a builtin base; modules read: 1 project, 2 foreign\nmembers: unresolved: requests.Session \u{d7}2 (no such module)\nmembers: unresolved: nope.Thing \u{d7}1 (no such name)\n",
            root.join(".venv").display(),
            stdlib.display(),
        )
    );
    Ok(())
}

fn trial(spec: &str, cases: &str) -> Result<Out, Fault> {
    let rules = format!("{}  cases:\n{cases}", rule(spec, "src/**/*.py"));
    Project::with_rules("[\"src\"]", &rules)?.run(&["test"])
}

#[test]
fn members_trial_a_base_in_the_case_file_resolves() -> Outcome {
    let out = trial(
        LINEAGE,
        "    valid: [\"class A:\\n    def run(self): ...\\n\\n\\nclass B(A):\\n    def _run_x(self): ...\\n\"]\n    invalid: [\"class A:\\n    def run(self): ...\\n\\n\\nclass B(A):\\n    def _go_x(self): ...\\n\"]\n",
    )?;
    assert_eq!(out.stdout, "Tested 1 rule, 2 cases: 0 failed, 0 untested.\n", "{out:?}");
    assert_eq!(out.code, 0);
    Ok(())
}

#[test]
fn members_trial_a_base_in_another_module_is_unresolved_so_an_invalid_case_fails() -> Outcome {
    let out = trial(
        LINEAGE,
        "    valid: [\"from other import A\\n\\n\\nclass B(A):\\n    def _stray(self): ...\\n\"]\n    invalid: [\"from other import A\\n\\n\\nclass B(A):\\n    def _stray(self): ...\\n\"]\n",
    )?;
    assert_eq!(
        out.stdout,
        "t.members: invalid case 1: expected a finding, found none\n\nTested 1 rule, 2 cases: 1 failed, 0 untested.\n"
    );
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn members_trial_order_cases_run() -> Outcome {
    let out = trial(
        ORDER,
        "    valid: [\"class A:\\n    def _a(self): ...\\n    def run(self): ...\\n\"]\n    invalid: [\"class A:\\n    def run(self): ...\\n    def _a(self): ...\\n\"]\n",
    )?;
    assert_eq!(out.stdout, "Tested 1 rule, 2 cases: 0 failed, 0 untested.\n");
    Ok(())
}

#[test]
fn members_private_after_a_public_breaks_the_order() -> Outcome {
    let code = "class A:\n    def _first(self): ...\n    def run(self): ...\n    def _late(self): ...\n    async def _later(self): ...\n    def go(self): ...\n";
    assert_eq!(
        single(ORDER, code)?,
        [
            here(4, "_late comes after public run"),
            here(5, "_later comes after public run")
        ]
    );
    Ok(())
}

#[test]
fn members_trial_cases_sharing_a_path_do_not_share_their_bases() -> Outcome {
    let out = trial(
        LINEAGE,
        "    valid:\n      - path: src/pkg/m.py\n        code: \"class A:\\n    def run(self): ...\\n\\n\\nclass B(A):\\n    def _run_x(self): ...\\n\"\n    invalid:\n      - path: src/pkg/m.py\n        code: \"class A:\\n    def go(self): ...\\n\\n\\nclass B(A):\\n    def _run_x(self): ...\\n\"\n",
    )?;
    assert_eq!(
        (out.stdout.as_str(), out.code),
        ("Tested 1 rule, 2 cases: 0 failed, 0 untested.\n", 0),
        "the invalid case's A has no run: judged against its own A, not the valid case's"
    );
    Ok(())
}

#[test]
fn members_order_holds_when_privates_come_first() -> Outcome {
    let code =
        "class A:\n    def _a(self): ...\n    def _b(self): ...\n    def run(self): ...\n    def go(self): ...\n";
    assert!(single(ORDER, code)?.is_empty());
    Ok(())
}

#[test]
fn members_order_ignores_dunders_whichever_side_they_sit() -> Outcome {
    let code = "class A:\n    def __init__(self): ...\n    def _a(self): ...\n    def __repr__(self): ...\n    def run(self): ...\n    def __eq__(self, other): ...\n";
    assert!(single(ORDER, code)?.is_empty());
    Ok(())
}

#[test]
fn members_order_is_judged_per_class() -> Outcome {
    let code = "class A:\n    def run(self): ...\n\n\nclass B:\n    def _a(self): ...\n";
    assert!(single(ORDER, code)?.is_empty());
    Ok(())
}

#[test]
fn members_order_alone_does_not_judge_lineage_and_lineage_alone_not_the_order() -> Outcome {
    let code = "class A:\n    def run(self): ...\n    def _stray(self): ...\n";
    assert_eq!(single(ORDER, code)?, [here(3, "_stray comes after public run")]);
    assert_eq!(single(LINEAGE, code)?, [here(3, "_stray serves no public of A")]);
    assert_eq!(
        single(BOTH, code)?,
        [
            here(3, "_stray comes after public run"),
            here(3, "_stray serves no public of A")
        ]
    );
    Ok(())
}

#[test]
fn members_order_alone_counts_judged_and_skipped_classes_in_the_census() -> Outcome {
    let code = "from requests import Session\n\n\nclass A: ...\n\n\nclass B(Session): ...\n";
    let seen = within(ORDER, &[("src/pkg/m.py", code)])?;
    assert_eq!(
        seen.notes[1],
        "members: 1 classes judged, 1 skipped (unresolved base), 0 on a builtin base; modules read: 0 project, 0 foreign"
    );
    Ok(())
}

#[test]
fn members_order_of_a_nested_class_is_its_own() -> Outcome {
    let code =
        "class A:\n    def run(self): ...\n\n    class B:\n        def go(self): ...\n        def _late(self): ...\n";
    assert_eq!(single(ORDER, code)?, [here(6, "_late comes after public go")]);
    Ok(())
}

#[test]
fn members_a_rule_judges_only_the_files_of_its_scope() -> Outcome {
    let code = "class A:\n    def _stray(self): ...\n";
    let seen = scene(
        LINEAGE,
        "[\"src\"]",
        "src/pkg/**/*.py",
        &[("src/pkg/in.py", code), ("src/other/out.py", code)],
        None,
    )?;
    assert_eq!(seen.found, [at("src/pkg/in.py", 2, "_stray serves no public of A")]);
    Ok(())
}

#[test]
fn members_a_stderr_without_a_run_fault_carries_only_census_lines_under_verbose() -> Outcome {
    let seen = within(LINEAGE, &[("src/pkg/m.py", "class A: ...\n")])?;
    assert!(
        seen.stderr
            .lines()
            .all(|line| line.starts_with("members:") || line.starts_with("time: ")),
        "{}",
        seen.stderr
    );
    assert_eq!(seen.code, 0);
    Ok(())
}
