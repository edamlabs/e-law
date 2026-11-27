mod common;

use std::time::Instant;

use common::{Out, Outcome, Project, found, noun, passed};
use serde_json::Value;

const ROOTS: &str = "[\"src\"]";
const ORPHAN_ENTRIES: &str = "entries: [\"src/*/main.py\", \"**/__main__.py\", \"tests/**\"]";

fn rule(id: &str, check: &str, scope: &str) -> String {
    let message = id.trim_start_matches("t.");
    format!(
        "- id: {id}\n  message: {message}\n{scope}  check: {{engine: graph, {check}}}\n  fix: {{safety: none, reason: x}}\n"
    )
}

fn cycle_rule(typing: bool) -> String {
    rule("t.cycle", &format!("aspect: cycle, typing: {typing}"), "")
}

fn orphan_rule() -> String {
    rule("t.orphan", &format!("aspect: orphan, {ORPHAN_ENTRIES}"), "")
}

fn project(roots: &str, rules: &str, files: &[(&str, &str)]) -> Result<Project, Box<dyn std::error::Error>> {
    let project = Project::with_rules(roots, rules)?;
    for (path, code) in files {
        project.file(path, code)?;
    }
    Ok(project)
}

fn report(lines: &[&str]) -> String {
    format!(
        "{}\n\nFound {} ({}, 0 warnings).\n",
        lines.join("\n"),
        noun(lines.len(), "finding", "findings"),
        noun(lines.len(), "error", "errors"),
    )
}

fn cycles(files: &[(&str, &str)]) -> Result<Out, Box<dyn std::error::Error>> {
    project(ROOTS, &cycle_rule(false), files)?.check(&[])
}

fn orphans(files: &[(&str, &str)]) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let out = project(ROOTS, &orphan_rule(), files)?.check(&[])?;
    Ok(out
        .stdout
        .lines()
        .filter_map(|line| line.split_once(":1:1: t.orphan orphan: nothing imports it"))
        .map(|(path, _)| path.to_owned())
        .collect())
}

fn orphans_in(roots: &str, files: &[(&str, &str)]) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let out = project(roots, &orphan_rule(), files)?.check(&[])?;
    Ok(out
        .stdout
        .lines()
        .filter_map(|line| line.split_once(":1:1: t.orphan orphan: nothing imports it"))
        .map(|(path, _)| path.to_owned())
        .collect())
}

fn extent(project: &Project) -> Result<String, Box<dyn std::error::Error>> {
    let out = project.check(&["-v"])?;
    out.stderr
        .lines()
        .find(|line| line.starts_with("graph: "))
        .map(str::to_owned)
        .ok_or_else(|| format!("no graph line: {}", out.stderr).into())
}

#[test]
fn graph_cycle_two_modules_is_one_finding_at_the_import_of_the_first() -> Outcome {
    let out = cycles(&[
        ("src/pkg/a.py", "x = 1\nimport pkg.b\n"),
        ("src/pkg/b.py", "from pkg import a\n"),
    ])?;
    assert_eq!(
        out.stdout,
        found(
            "src/pkg/a.py:2:1",
            "t.cycle",
            "cycle: pkg.a \u{2192} pkg.b \u{2192} pkg.a",
            1,
            0
        )
    );
    assert_eq!((out.stderr.as_str(), out.code), ("", 1));
    Ok(())
}

#[test]
fn graph_cycle_three_modules_names_the_whole_chain() -> Outcome {
    let out = cycles(&[
        ("src/pkg/a.py", "import pkg.b\n"),
        ("src/pkg/b.py", "import pkg.c\n"),
        ("src/pkg/c.py", "import pkg.a\n"),
    ])?;
    assert_eq!(
        out.stdout,
        found(
            "src/pkg/a.py:1:1",
            "t.cycle",
            "cycle: pkg.a \u{2192} pkg.b \u{2192} pkg.c \u{2192} pkg.a",
            1,
            0
        )
    );
    Ok(())
}

#[test]
fn graph_cycle_self_import_is_a_cycle_of_one() -> Outcome {
    let out = cycles(&[("src/pkg/a.py", "x = 1\nimport pkg.a\n"), ("src/pkg/b.py", "x = 1\n")])?;
    assert_eq!(
        out.stdout,
        found("src/pkg/a.py:2:1", "t.cycle", "cycle: pkg.a \u{2192} pkg.a", 1, 0)
    );
    Ok(())
}

#[test]
fn graph_cycle_two_disjoint_cycles_are_two_findings_each_with_its_chain() -> Outcome {
    let out = cycles(&[
        ("src/pkg/a.py", "import pkg.b\n"),
        ("src/pkg/b.py", "import pkg.a\n"),
        ("src/pkg/p.py", "import pkg.q\n"),
        ("src/pkg/q.py", "x = 1\nimport pkg.r\n"),
        ("src/pkg/r.py", "import pkg.p\n"),
    ])?;
    assert_eq!(
        out.stdout,
        report(&[
            "src/pkg/a.py:1:1: t.cycle cycle: pkg.a \u{2192} pkg.b \u{2192} pkg.a",
            "src/pkg/p.py:1:1: t.cycle cycle: pkg.p \u{2192} pkg.q \u{2192} pkg.r \u{2192} pkg.p",
        ])
    );
    Ok(())
}

#[test]
fn graph_cycle_one_component_with_two_loops_is_one_finding_the_shortest() -> Outcome {
    let out = cycles(&[
        ("src/pkg/a.py", "import pkg.c\nimport pkg.b\n"),
        ("src/pkg/b.py", "import pkg.a\n"),
        ("src/pkg/c.py", "import pkg.d\n"),
        ("src/pkg/d.py", "import pkg.a\n"),
    ])?;
    assert_eq!(
        out.stdout,
        found(
            "src/pkg/a.py:2:1",
            "t.cycle",
            "cycle: pkg.a \u{2192} pkg.b \u{2192} pkg.a",
            1,
            0
        )
    );
    Ok(())
}

#[test]
fn graph_cycle_closed_by_an_import_inside_a_function_is_found() -> Outcome {
    let out = cycles(&[
        ("src/pkg/a.py", "import pkg.b\n"),
        ("src/pkg/b.py", "def run() -> None:\n    from pkg import a\n"),
    ])?;
    assert_eq!(
        out.stdout,
        found(
            "src/pkg/a.py:1:1",
            "t.cycle",
            "cycle: pkg.a \u{2192} pkg.b \u{2192} pkg.a",
            1,
            0
        )
    );
    Ok(())
}

#[test]
fn graph_cycle_through_a_from_package_import_of_a_module_is_found() -> Outcome {
    let out = cycles(&[
        ("src/pkg/__init__.py", ""),
        ("src/pkg/a.py", "from pkg import b\n"),
        ("src/pkg/b.py", "from pkg.a import A\n"),
    ])?;
    assert_eq!(
        out.stdout,
        found(
            "src/pkg/a.py:1:1",
            "t.cycle",
            "cycle: pkg.a \u{2192} pkg.b \u{2192} pkg.a",
            1,
            0
        )
    );
    Ok(())
}

#[test]
fn graph_cycle_from_package_import_of_a_name_is_an_edge_to_the_package() -> Outcome {
    let out = cycles(&[
        ("src/pkg/__init__.py", "from pkg.a import A\n"),
        ("src/pkg/a.py", "from pkg import Thing\n"),
    ])?;
    assert_eq!(
        out.stdout,
        found(
            "src/pkg/__init__.py:1:1",
            "t.cycle",
            "cycle: pkg \u{2192} pkg.a \u{2192} pkg",
            1,
            0
        )
    );
    Ok(())
}

#[test]
fn graph_cycle_through_a_dotted_import_is_an_edge_to_the_last_module() -> Outcome {
    let out = cycles(&[
        ("src/pkg/x.py", "import pkg.sub.c\n"),
        ("src/pkg/sub/c.py", "import pkg.x\n"),
    ])?;
    assert_eq!(
        out.stdout,
        found(
            "src/pkg/sub/c.py:1:1",
            "t.cycle",
            "cycle: pkg.sub.c \u{2192} pkg.x \u{2192} pkg.sub.c",
            1,
            0
        )
    );
    Ok(())
}

#[test]
fn graph_cycle_through_relative_imports_is_found_in_each_form() -> Outcome {
    for (name, a, b) in [
        ("from .x import", "from .b import B\n", "from .a import A\n"),
        ("from ..x import", "from ..m.b import B\n", "from ..m.a import A\n"),
        ("from . import x", "from . import b\n", "from . import a\n"),
    ] {
        let out = cycles(&[
            ("src/pkg/m/a.py", a),
            ("src/pkg/m/b.py", b),
            ("src/pkg/__init__.py", ""),
        ])?;
        assert_eq!(
            out.stdout,
            found(
                "src/pkg/m/a.py:1:1",
                "t.cycle",
                "cycle: pkg.m.a \u{2192} pkg.m.b \u{2192} pkg.m.a",
                1,
                0
            ),
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn graph_cycle_relative_import_beyond_the_top_is_no_edge_and_no_fault() -> Outcome {
    let out = cycles(&[
        ("src/pkg/a.py", "from ... import b\n"),
        ("src/pkg/b.py", "import pkg.a\n"),
    ])?;
    assert_eq!(
        (out.stdout.as_str(), out.stderr.as_str(), out.code),
        ("All checks passed.\n", "", 0)
    );
    Ok(())
}

#[test]
fn graph_cycle_across_two_roots_is_found() -> Outcome {
    let project = project(
        "[\"src\", \"lib\"]",
        &cycle_rule(false),
        &[
            ("src/pkg/a.py", "import other.b\n"),
            ("lib/other/b.py", "import pkg.a\n"),
        ],
    )?;
    assert_eq!(
        project.check(&[])?.stdout,
        found(
            "lib/other/b.py:1:1",
            "t.cycle",
            "cycle: other.b \u{2192} pkg.a \u{2192} other.b",
            1,
            0
        )
    );
    Ok(())
}

#[test]
fn graph_cycle_through_the_project_root_is_resolved_for_a_second_root() -> Outcome {
    let project = project(
        "[\"src\", \"tests\"]",
        &cycle_rule(false),
        &[
            ("src/pkg/a.py", "import tests.unit.x\n"),
            ("tests/unit/x.py", "import pkg.a\n"),
        ],
    )?;
    let out = project.check(&[])?;
    assert_eq!(out.code, 1, "{}", out.stdout);
    assert!(
        out.stdout
            .starts_with("src/pkg/a.py:1:1: t.cycle cycle: pkg.a \u{2192} "),
        "{}",
        out.stdout
    );
    Ok(())
}

#[test]
fn graph_cycle_names_a_module_of_a_second_root_as_its_importers_write_it() -> Outcome {
    let project = project(
        "[\"src\", \"tests\"]",
        &cycle_rule(false),
        &[
            ("src/pkg/a.py", "import tests.unit.x\n"),
            ("tests/unit/x.py", "import pkg.a\n"),
        ],
    )?;
    assert_eq!(
        project.check(&[])?.stdout,
        found(
            "src/pkg/a.py:1:1",
            "t.cycle",
            "cycle: pkg.a \u{2192} tests.unit.x \u{2192} pkg.a",
            1,
            0
        )
    );
    Ok(())
}

#[test]
fn graph_cycle_two_roots_holding_one_dotted_name_name_the_first_root_alone() -> Outcome {
    let project = project(
        "[\"src\", \"tests\"]",
        &cycle_rule(false),
        &[
            ("src/pkg/a.py", "import tests.pkg.a\n"),
            ("tests/pkg/a.py", "import pkg.a\n"),
        ],
    )?;
    assert_eq!(
        project.check(&[])?.stdout,
        found(
            "src/pkg/a.py:1:1",
            "t.cycle",
            "cycle: pkg.a \u{2192} tests.pkg.a \u{2192} pkg.a",
            1,
            0
        )
    );
    Ok(())
}

const DOT_ROOTS: &str = "[\".\"]";

#[test]
fn graph_src_layout_with_the_default_root_resolves_imports_written_without_src() -> Outcome {
    let out = project(
        DOT_ROOTS,
        &orphan_rule(),
        &[
            ("src/pkg/main.py", "import pkg.app\n"),
            ("src/pkg/app.py", "x = 1\n"),
            ("src/pkg/dead.py", "x = 2\n"),
        ],
    )?
    .check(&[])?;
    assert_eq!(
        out.stdout,
        found("src/pkg/dead.py:1:1", "t.orphan", "orphan: nothing imports it", 1, 0)
    );
    Ok(())
}

#[test]
fn graph_src_layout_names_a_module_without_src_and_a_module_beside_src_by_its_path() -> Outcome {
    let out = project(
        DOT_ROOTS,
        &cycle_rule(false),
        &[("src/pkg/a.py", "import tools.b\n"), ("tools/b.py", "import pkg.a\n")],
    )?
    .check(&[])?;
    assert_eq!(
        out.stdout,
        found(
            "src/pkg/a.py:1:1",
            "t.cycle",
            "cycle: pkg.a \u{2192} tools.b \u{2192} pkg.a",
            1,
            0
        )
    );
    Ok(())
}

#[test]
fn graph_src_layout_still_resolves_an_import_written_with_src() -> Outcome {
    assert_eq!(
        orphans_in(
            DOT_ROOTS,
            &[
                ("src/pkg/main.py", "import src.pkg.app\n"),
                ("src/pkg/app.py", "x = 1\n"),
            ]
        )?,
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn graph_src_layout_does_not_walk_src_when_it_is_not_a_configured_root() -> Outcome {
    let project = project(
        "[\"lib\"]",
        &orphan_rule(),
        &[("lib/a.py", "x = 1\n"), ("src/pkg/b.py", "x = 2\n")],
    )?;
    assert_eq!(
        project.check(&[])?.stdout,
        found("lib/a.py:1:1", "t.orphan", "orphan: nothing imports it", 1, 0)
    );
    Ok(())
}

#[test]
fn graph_paths_a_cycle_none_of_whose_modules_is_passed_is_not_reported() -> Outcome {
    let project = project(
        ROOTS,
        &cycle_rule(false),
        &[
            ("src/pkg/a.py", "import pkg.b\n"),
            ("src/pkg/b.py", "import pkg.a\n"),
            ("src/pkg/c.py", "x = 1\n"),
        ],
    )?;
    assert!(passed(&project.check(&["src/pkg/c.py"])?));
    Ok(())
}

#[test]
fn graph_cycle_third_party_and_standard_imports_are_no_nodes() -> Outcome {
    let project = project(
        ROOTS,
        &cycle_rule(false),
        &[
            (
                "src/pkg/a.py",
                "import os\nimport requests\nfrom numpy import array\nimport pkg.b\n",
            ),
            ("src/pkg/b.py", "import json\n"),
        ],
    )?;
    assert_eq!(extent(&project)?, "graph: 2 modules, 1 edges (0 typing-only)");
    assert!(passed(&project.check(&[])?));
    Ok(())
}

#[test]
fn graph_cycle_a_module_outside_every_root_is_no_node() -> Outcome {
    let project = project(
        ROOTS,
        &cycle_rule(false),
        &[
            ("src/pkg/a.py", "import pkg.b\n"),
            ("src/pkg/b.py", "x = 1\n"),
            ("scripts/s.py", "import pkg.a\n"),
        ],
    )?;
    assert_eq!(extent(&project)?, "graph: 2 modules, 1 edges (0 typing-only)");
    Ok(())
}

const TYPING_ONLY_A: &str = "from typing import TYPE_CHECKING\n\nif TYPE_CHECKING:\n    from pkg import b\n";

#[test]
fn graph_typing_cycle_through_type_checking_only_is_ignored_by_default() -> Outcome {
    let project = project(
        ROOTS,
        &cycle_rule(false),
        &[("src/pkg/a.py", TYPING_ONLY_A), ("src/pkg/b.py", "import pkg.a\n")],
    )?;
    assert!(passed(&project.check(&[])?));
    assert_eq!(extent(&project)?, "graph: 2 modules, 2 edges (1 typing-only)");
    Ok(())
}

#[test]
fn graph_typing_cycle_through_type_checking_is_found_when_the_rule_counts_it() -> Outcome {
    let project = project(
        ROOTS,
        &cycle_rule(true),
        &[("src/pkg/a.py", TYPING_ONLY_A), ("src/pkg/b.py", "import pkg.a\n")],
    )?;
    assert_eq!(
        project.check(&[])?.stdout,
        found(
            "src/pkg/a.py:4:5",
            "t.cycle",
            "cycle: pkg.a \u{2192} pkg.b \u{2192} pkg.a",
            1,
            0
        )
    );
    Ok(())
}

#[test]
fn graph_typing_attribute_form_and_the_else_branch_follow_the_condition() -> Outcome {
    let project = project(
        ROOTS,
        &cycle_rule(false),
        &[
            (
                "src/pkg/a.py",
                "import typing\n\nif typing.TYPE_CHECKING:\n    from pkg import b\nelse:\n    from pkg import b\n",
            ),
            ("src/pkg/b.py", "import pkg.a\n"),
        ],
    )?;
    assert_eq!(
        project.check(&[])?.stdout,
        found(
            "src/pkg/a.py:6:5",
            "t.cycle",
            "cycle: pkg.a \u{2192} pkg.b \u{2192} pkg.a",
            1,
            0
        )
    );
    Ok(())
}

#[test]
fn graph_typing_an_edge_with_one_plain_statement_is_plain_and_sits_at_that_line() -> Outcome {
    let project = project(
        ROOTS,
        &cycle_rule(false),
        &[
            (
                "src/pkg/a.py",
                "from typing import TYPE_CHECKING\n\nif TYPE_CHECKING:\n    from pkg import b\n\ndef run() -> None:\n    import pkg.b\n",
            ),
            ("src/pkg/b.py", "import pkg.a\n"),
        ],
    )?;
    assert_eq!(
        project.check(&[])?.stdout,
        found(
            "src/pkg/a.py:7:5",
            "t.cycle",
            "cycle: pkg.a \u{2192} pkg.b \u{2192} pkg.a",
            1,
            0
        )
    );
    assert_eq!(extent(&project)?, "graph: 2 modules, 2 edges (0 typing-only)");
    Ok(())
}

const A_B_C: [(&str, &str); 3] = [
    ("src/pkg/a.py", "import pkg.b\n"),
    ("src/pkg/b.py", "import pkg.a\n"),
    ("src/pkg/c.py", "x = 1\n"),
];

#[test]
fn graph_paths_the_first_module_of_a_cycle_passed_alone_finds_it() -> Outcome {
    let project = project(ROOTS, &cycle_rule(false), &A_B_C)?;
    let out = project.check(&["src/pkg/a.py"])?;
    assert_eq!(
        out.stdout,
        found(
            "src/pkg/a.py:1:1",
            "t.cycle",
            "cycle: pkg.a \u{2192} pkg.b \u{2192} pkg.a",
            1,
            0
        )
    );
    Ok(())
}

#[test]
fn graph_paths_an_uninvolved_module_passed_alone_reports_nothing() -> Outcome {
    let project = project(ROOTS, &cycle_rule(false), &A_B_C)?;
    assert!(passed(&project.check(&["src/pkg/c.py"])?));
    assert_eq!(extent(&project)?, "graph: 3 modules, 2 edges (0 typing-only)");
    Ok(())
}

#[test]
fn graph_paths_the_graph_is_whole_whatever_the_paths_are() -> Outcome {
    let project = project(ROOTS, &cycle_rule(false), &A_B_C)?;
    let out = project.check(&["-v", "src/pkg/c.py"])?;
    assert!(
        out.stderr
            .lines()
            .any(|line| line == "graph: 3 modules, 2 edges (0 typing-only)"),
        "{}",
        out.stderr
    );
    Ok(())
}

#[test]
fn graph_paths_any_module_of_a_cycle_passed_alone_sees_it() -> Outcome {
    let project = project(ROOTS, &cycle_rule(false), &A_B_C)?;
    let out = project.check(&["src/pkg/b.py"])?;
    assert_eq!(
        out.code, 1,
        "a cycle closed by editing only b.py passed a pre-commit that gave only b.py: {out:?}"
    );
    Ok(())
}

#[test]
fn graph_scope_filters_the_carriers_of_a_cycle_not_its_modules() -> Outcome {
    let scoped = |scope: &str| -> Result<Out, Box<dyn std::error::Error>> {
        project(ROOTS, &rule("t.cycle", "aspect: cycle", scope), &A_B_C)?.check(&[])
    };
    let only_b = scoped("  scope: {include: [\"src/pkg/b.py\"]}\n")?;
    assert_eq!(
        only_b.stdout,
        found(
            "src/pkg/b.py:1:1",
            "t.cycle",
            "cycle: pkg.b \u{2192} pkg.a \u{2192} pkg.b",
            1,
            0
        )
    );
    assert!(passed(&scoped("  scope: {include: [\"src/pkg/c.py\"]}\n")?));
    assert!(passed(&scoped("  scope: {exclude: [\"src/**\"]}\n")?));
    Ok(())
}

#[test]
fn graph_scope_an_importer_out_of_scope_still_keeps_a_module_from_being_an_orphan() -> Outcome {
    let rules = rule(
        "t.orphan",
        &format!("aspect: orphan, {ORPHAN_ENTRIES}"),
        "  scope: {include: [\"src/pkg/app.py\"]}\n",
    );
    let project = project(
        ROOTS,
        &rules,
        &[("src/pkg/main.py", "import pkg.app\n"), ("src/pkg/app.py", "x = 1\n")],
    )?;
    assert!(passed(&project.check(&[])?));
    Ok(())
}

#[test]
fn graph_orphan_a_module_nothing_imports_is_a_finding_at_line_1() -> Outcome {
    let out = project(
        ROOTS,
        &orphan_rule(),
        &[
            ("src/pkg/main.py", "import pkg.app\n"),
            ("src/pkg/app.py", "x = 1\n"),
            ("src/pkg/dead.py", "\n\ny = 2\n"),
        ],
    )?
    .check(&[])?;
    assert_eq!(
        out.stdout,
        found("src/pkg/dead.py:1:1", "t.orphan", "orphan: nothing imports it", 1, 0)
    );
    Ok(())
}

#[test]
fn graph_orphan_the_entries_are_doors_and_never_judged() -> Outcome {
    assert_eq!(
        orphans(&[
            ("src/pkg/main.py", "x = 1\n"),
            ("src/pkg/__main__.py", "x = 1\n"),
            ("tests/unit/test_a.py", "x = 1\n"),
            ("src/pkg/other.py", "x = 1\n"),
        ])?,
        ["src/pkg/other.py"]
    );
    Ok(())
}

#[test]
fn graph_orphan_a_package_init_is_imported_with_the_package_or_anything_inside_it() -> Outcome {
    assert_eq!(
        orphans(&[
            ("src/a/__init__.py", ""),
            ("src/b/__init__.py", ""),
            ("src/b/sub/__init__.py", ""),
            ("src/b/sub/leaf.py", "x = 1\n"),
            ("src/c/__init__.py", ""),
            ("src/c/inner.py", "x = 1\n"),
            ("src/pkg/main.py", "import a\nimport b.sub.leaf\n"),
        ])?,
        ["src/c/__init__.py", "src/c/inner.py"]
    );
    Ok(())
}

#[test]
fn graph_orphan_a_from_package_import_module_imports_the_module_and_the_package() -> Outcome {
    assert_eq!(
        orphans(&[
            ("src/pkg/__init__.py", ""),
            ("src/pkg/mod.py", "x = 1\n"),
            ("src/pkg/main.py", "from pkg import mod\n"),
        ])?,
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn graph_orphan_a_module_imported_only_by_itself_is_one() -> Outcome {
    assert_eq!(
        orphans(&[("src/pkg/main.py", "x = 1\n"), ("src/pkg/loop.py", "import pkg.loop\n")])?,
        ["src/pkg/loop.py"]
    );
    Ok(())
}

#[test]
fn graph_orphan_a_module_imported_only_by_an_orphan_is_not_one() -> Outcome {
    assert_eq!(
        orphans(&[
            ("src/pkg/main.py", "x = 1\n"),
            ("src/pkg/dead.py", "import pkg.leaf\n"),
            ("src/pkg/leaf.py", "x = 1\n"),
        ])?,
        ["src/pkg/dead.py"]
    );
    Ok(())
}

#[test]
fn graph_orphan_a_typing_only_importer_still_imports() -> Outcome {
    assert_eq!(
        orphans(&[("src/pkg/main.py", TYPING_ONLY_A), ("src/pkg/b.py", "x = 1\n")])?,
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn graph_orphan_tests_import_the_source_only_when_tests_is_a_root() -> Outcome {
    let files = [
        ("src/pkg/a.py", "x = 1\n"),
        ("tests/unit/test_a.py", "from pkg import a\n"),
    ];
    let with = project("[\"src\", \"tests\"]", &orphan_rule(), &files)?.check(&[])?;
    assert!(passed(&with), "{with:?}");
    let without = project(ROOTS, &orphan_rule(), &files)?.check(&[])?;
    assert_eq!(
        without.stdout,
        found("src/pkg/a.py:1:1", "t.orphan", "orphan: nothing imports it", 1, 0)
    );
    Ok(())
}

fn many() -> Vec<(String, String)> {
    (0..40)
        .map(|index| {
            let next = (index + 1) % 8 + index / 8 * 8;
            (format!("src/pkg/m{index}.py"), format!("import pkg.m{next}\n"))
        })
        .collect()
}

#[test]
fn graph_output_is_the_same_across_three_runs() -> Outcome {
    let files = many();
    let borrowed: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    let project = project(
        ROOTS,
        &format!("{}{}", cycle_rule(false), orphan_rule().replace("t.orphan", "t.orph")),
        &borrowed,
    )?;
    let runs: Vec<String> = (0..3)
        .map(|_| project.check(&[]).map(|out| out.stdout))
        .collect::<Result<_, _>>()?;
    assert_eq!(runs[0], runs[1]);
    assert_eq!(runs[1], runs[2]);
    let findings: Vec<Value> = (0..3)
        .map(|_| project.json(&[]).map(|report| report["findings"].clone()))
        .collect::<Result<_, _>>()?;
    assert_eq!(findings[0], findings[1]);
    assert_eq!(findings[1], findings[2]);
    assert_eq!(findings[0].as_array().map(Vec::len), Some(5));
    Ok(())
}

#[test]
fn graph_output_verbose_extent_and_phase_times_are_on_stderr() -> Outcome {
    let project = project(ROOTS, &cycle_rule(false), &A_B_C)?;
    let started = Instant::now();
    let out = project.check(&["-v"])?;
    let wall = started.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(out.code, 1);
    assert!(
        !out.stdout.contains("graph:") && !out.stdout.contains("time:"),
        "{}",
        out.stdout
    );
    let times: Vec<(&str, f64)> = out
        .stderr
        .lines()
        .filter_map(|line| line.strip_prefix("time: "))
        .filter_map(|line| line.split_once(' '))
        .map(|(name, ms)| ms.parse::<f64>().map(|ms| (name, ms)))
        .collect::<Result<_, _>>()?;
    let names: Vec<&str> = times.iter().map(|(name, _)| *name).collect();
    assert_eq!(names, ["probe", "walk", "parse", "graph", "tree"], "{}", out.stderr);
    assert!(
        out.stderr
            .lines()
            .any(|line| line == "graph: 3 modules, 2 edges (0 typing-only)"),
        "{}",
        out.stderr
    );
    let sum: f64 = times.iter().map(|(_, ms)| ms).sum();
    assert!(sum <= wall, "phases {sum} ms against a wall of {wall} ms");
    Ok(())
}

#[test]
fn graph_output_verbose_says_nothing_of_a_graph_when_no_rule_judges_one() -> Outcome {
    let project = Project::with_rules(ROOTS, common::ELIF)?;
    project.file("src/pkg/a.py", common::WITH_ELIF)?;
    let out = project.check(&["-v"])?;
    assert!(!out.stderr.contains("graph:"), "{}", out.stderr);
    assert!(
        !out.stderr.lines().any(|line| line.starts_with("time: graph")),
        "{}",
        out.stderr
    );
    Ok(())
}

const ENGINES: &str = "- id: t.pat
  message: m
  check: {engine: pattern, rule: {kind: elif_clause}}
  fix: {safety: none, reason: x}
- id: t.mem
  message: m
  check: {engine: members, lineage: {common: [common]}, order: privates-first}
  fix: {safety: none, reason: x}
- id: t.lineage
  message: m
  check: {engine: members, lineage: {common: [common]}}
  fix: {safety: none, reason: x}
- id: t.shape
  message: m
  check: {engine: shape, classes: {count: 1}, functions: {max: 1}, constants: {max: 1}}
  fix: {safety: none, reason: x}
- id: t.tree
  message: m
  check: {engine: tree, dirs: [\"src/*\", \"tests/*\"]}
  fix: {safety: none, reason: x}
- id: t.tach
  message: m
  check: {engine: tach, code: LayerViolation}
  fix: {safety: none, reason: x}
- id: t.ruff
  message: m
  check: {engine: ruff, code: T201}
  fix: {safety: none, reason: x}
- id: t.ty
  message: m
  check: {engine: ty}
  fix: {safety: none, reason: x}
- id: t.cycle
  message: cycle
  check: {engine: graph, aspect: cycle}
  fix: {safety: none, reason: x}
- id: t.orphan
  message: orphan
  check: {engine: graph, aspect: orphan, entries: [\"src/*/main.py\"]}
  fix: {safety: none, reason: x}
";

#[test]
fn graph_rule_text_shows_the_parts_of_each_engine() -> Outcome {
    let project = Project::with_rules(ROOTS, ENGINES)?;
    for (id, engine) in [
        ("t.pat", "pattern"),
        ("t.mem", "members lineage, order"),
        ("t.lineage", "members lineage"),
        ("t.shape", "shape classes, functions, constants"),
        ("t.tree", "tree src/*, tests/*"),
        ("t.tach", "tach LayerViolation"),
        ("t.ruff", "ruff T201"),
        ("t.ty", "ty"),
        ("t.cycle", "graph cycle"),
        ("t.orphan", "graph orphan entries src/*/main.py"),
    ] {
        let out = project.run(&["rule", id])?;
        assert_eq!((out.stderr.as_str(), out.code), ("", 0), "{id}");
        assert_eq!(
            out.stdout.lines().nth(4),
            Some(format!("engine: {engine}").as_str()),
            "{id}: {}",
            out.stdout
        );
    }
    Ok(())
}

#[test]
fn graph_rule_text_of_a_graph_rule_is_exact() -> Outcome {
    let project = Project::with_rules(ROOTS, ENGINES)?;
    assert_eq!(
        project.run(&["rule", "t.orphan"])?.stdout,
        "id: t.orphan\nmessage: orphan\ntags: none\nseverity: error\nengine: graph orphan entries src/*/main.py\nfix: none (x)\nscope: include **/*.py\nvalid: none\ninvalid: none\nfixed: none\n"
    );
    Ok(())
}

#[test]
fn graph_rule_text_of_a_profile_rule_lists_its_kinds_and_aspect() -> Outcome {
    let out = Project::new()?.run(&["rule", "natures.provider-present", "--catalogue", "eager"])?;
    assert_eq!(
        out.stdout.lines().nth(4),
        Some("engine: profile driver, adapter present"),
        "{}",
        out.stdout
    );
    Ok(())
}

#[test]
fn graph_rule_text_shows_the_files_of_a_files_case() -> Outcome {
    let out = Project::new()?.run(&["rule", "natures.no-import-cycle", "--catalogue", "eager"])?;
    assert_eq!(out.code, 0, "{out:?}");
    assert!(out.stdout.contains("src/pkg/a.py"), "{}", out.stdout);
    assert!(out.stdout.contains("src/pkg/b.py"), "{}", out.stdout);
    Ok(())
}

fn with_cases(cases: &str, check: &str) -> String {
    format!("- id: t.cycle\n  message: cycle\n  check: {check}\n  fix: {{safety: none, reason: x}}\n  cases:\n{cases}")
}

const GRAPH_CHECK: &str = "{engine: graph, aspect: cycle}";
const BAD_CASE: &str = "cat/rules/r.yml:7: bad-case: a case sets `code`, with an optional `path`, or a non-empty `files`, with no `path`\n";
const TWO: &str =
    "      - files:\n          src/pkg/a.py: \"import pkg.b\\n\"\n          src/pkg/b.py: \"import pkg.a\\n\"\n";
const ONE: &str = "      - files:\n          src/pkg/a.py: \"x = 1\\n\"\n";

fn tested(cases: &str, check: &str) -> Result<Out, Box<dyn std::error::Error>> {
    Project::with_rules(ROOTS, &with_cases(cases, check))?.run(&["test"])
}

#[test]
fn graph_case_files_valid_and_invalid_pass() -> Outcome {
    let out = tested(&format!("    valid:\n{ONE}    invalid:\n{TWO}"), GRAPH_CHECK)?;
    assert_eq!(
        (out.stdout.as_str(), out.stderr.as_str(), out.code),
        ("Tested 1 rule, 2 cases: 0 failed, 0 untested.\n", "", 0)
    );
    Ok(())
}

#[test]
fn graph_case_files_a_valid_case_that_finds_and_an_invalid_that_does_not_are_failures() -> Outcome {
    let out = tested(&format!("    valid:\n{TWO}    invalid:\n{ONE}"), GRAPH_CHECK)?;
    assert_eq!(
        out.stdout,
        "t.cycle: valid case 1: expected no finding, found 1 (the first at 1:1)\nt.cycle: invalid case 1: expected a finding, found none\n\nTested 1 rule, 2 cases: 2 failed, 0 untested.\n"
    );
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn graph_case_files_refusals_are_a_bad_case_naming_the_line() -> Outcome {
    for (name, cases) in [
        (
            "code and files",
            "    valid:\n      - code: \"x = 1\"\n        files: {src/pkg/a.py: \"x = 1\"}\n",
        ),
        ("neither", "    valid:\n      - path: src/pkg/a.py\n"),
        ("empty files", "    valid:\n      - files: {}\n"),
        (
            "path and files",
            "    valid:\n      - path: src/pkg/a.py\n        files: {src/pkg/a.py: \"x = 1\"}\n",
        ),
    ] {
        for command in [vec!["test"], vec!["check"]] {
            let out = Project::with_rules(ROOTS, &with_cases(cases, GRAPH_CHECK))?.run(&command)?;
            assert_eq!(
                (out.stdout.as_str(), out.stderr.as_str(), out.code),
                ("", BAD_CASE, 2),
                "{name} / {command:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn graph_case_files_on_another_engine_is_a_load_fault() -> Outcome {
    for (engine, check) in [
        ("pattern", "{engine: pattern, rule: {kind: elif_clause}}"),
        ("tree", "{engine: tree, dirs: [\"src/*\"]}"),
        ("ty", "{engine: ty}"),
    ] {
        for command in ["test", "check"] {
            let out = Project::with_rules(ROOTS, &with_cases(&format!("    valid:\n{ONE}"), check))?.run(&[command])?;
            assert_eq!(
                (out.stdout.as_str(), out.stderr.as_str(), out.code),
                (
                    "",
                    format!("cat/rules/r.yml: rule `t.cycle` runs on `{engine}`, which proves no `files` case\n")
                        .as_str(),
                    2
                ),
                "{engine} / {command}"
            );
        }
    }
    Ok(())
}

#[test]
fn graph_case_code_on_a_graph_rule_is_a_load_fault() -> Outcome {
    for cases in [
        "    valid: [\"x = 1\\n\"]\n",
        "    invalid:\n      - {path: src/pkg/a.py, code: \"import pkg.a\\n\"}\n",
    ] {
        for command in ["test", "check"] {
            let out = Project::with_rules(ROOTS, &with_cases(cases, GRAPH_CHECK))?.run(&[command])?;
            assert_eq!(
                (out.stdout.as_str(), out.stderr.as_str(), out.code),
                (
                    "",
                    "cat/rules/r.yml: rule `t.cycle` runs on `graph`, a graph rule is proven by `files` cases, not a `code` case\n",
                    2
                ),
                "{cases} / {command}"
            );
        }
    }
    Ok(())
}

#[test]
fn graph_case_code_snippets_and_files_may_sit_in_one_catalogue() -> Outcome {
    let rules = format!(
        "{}{}",
        with_cases(&format!("    valid:\n{ONE}"), GRAPH_CHECK),
        "- id: t.elif\n  message: no elif\n  check: {engine: pattern, rule: {kind: elif_clause}}\n  fix: {safety: none, reason: x}\n  cases:\n    valid: [\"x = 1\\n\"]\n    invalid: [\"if a:\\n    pass\\nelif b:\\n    pass\\n\"]\n"
    );
    let out = Project::with_rules(ROOTS, &rules)?.run(&["test"])?;
    assert_eq!(
        out.stdout,
        "t.cycle: invalid: no case: an unproven rule is not a rule\n\nTested 2 rules, 3 cases: 1 failed, 0 untested.\n"
    );
    Ok(())
}

#[test]
fn graph_case_files_run_in_a_project_of_their_own_apart_from_the_projects_roots() -> Outcome {
    let project = Project::with_rules(
        "[\"lib\"]",
        &with_cases(&format!("    invalid:\n{TWO}    valid:\n{ONE}"), GRAPH_CHECK),
    )?;
    project.file("lib/other/z.py", "import other.z\n")?;
    let out = project.run(&["test"])?;
    assert_eq!(out.stdout, "Tested 1 rule, 2 cases: 0 failed, 0 untested.\n", "{out:?}");
    Ok(())
}
