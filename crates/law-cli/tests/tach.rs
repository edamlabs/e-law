mod common;

use std::ffi::OsString;
use std::fs;
use std::path::Path;

use common::{Out, Outcome, Project, TACH, noun, only, passed, path_with, skip, stub};
use serde_json::Value;

const CAPTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../law-external/tests/fixtures/tach");
const PRAGMA: &str = "law.pragma silenced inline: register an exception instead";
const VERSION: &str = "0.35.3";
const EAGER: &str = "roots = [\"src\"]\ncatalogues = [\"eager\"]\n";
const LAYERS: &str = "layers = [\"ui\", \"core\", \"low\"]\n";
const STRICT: &str = "layers_explicit_depends_on = true\n";
const PYPROJECT: &str = "[project]\nname = \"pkg\"\nversion = \"0\"\ndependencies = [\"httpx\", \"pyyaml\"]\n";
const CLEAN: &str = "[]";
const EXTERNAL_OK: &str = "\u{2705} All external dependencies validated!\n";

const KINDS: [(&str, &str, &str, &str); 12] = [
    (
        "undeclared",
        "natures.declared-dependency",
        "src/pkg/ui/view.py:1:1",
        "pkg.ui imports pkg.low.base, undeclared",
    ),
    (
        "layer",
        "natures.layer-order",
        "src/pkg/core/model.py:2:1",
        "pkg.core (core) imports pkg.ui.view (ui), a higher layer",
    ),
    (
        "forbidden",
        "natures.no-forbidden-dependency",
        "src/pkg/ui/view.py:1:1",
        "pkg.ui imports pkg.low.base, forbidden",
    ),
    (
        "cycle",
        "natures.no-cycle",
        "tach.toml:1:1",
        "pkg.core, pkg.low depend on each other",
    ),
    (
        "visibility",
        "natures.visible-dependency",
        "tach.toml:1:1",
        "pkg.ui depends on pkg.core, outside its visibility (pkg.low)",
    ),
    (
        "interfaces",
        "natures.public-interface",
        "src/pkg/ui/view.py:1:1",
        "pkg.ui imports pkg.core.model.x, outside the interface of pkg.core",
    ),
    (
        "ignores",
        "natures.used-ignore",
        "src/pkg/ui/view.py:1:1",
        "a tach-ignore that ignores nothing: tach gives no line",
    ),
    (
        "ignores",
        "natures.ignore-reason",
        "src/pkg/ui/view.py:1:1",
        "a tach-ignore with no reason: tach gives no line",
    ),
    (
        "ext1",
        "natures.declared-external",
        "src/pkg/core/model.py:1:1",
        "requests is imported, undeclared in pkg",
    ),
    (
        "ext1",
        "natures.no-unused-external",
        "tach.toml:1:1",
        "httpx is declared in pkg, never imported",
    ),
    (
        "modext",
        "natures.declared-module-external",
        "src/pkg/core/model.py:1:1",
        "pkg.core imports requests, undeclared",
    ),
    (
        "modext",
        "natures.no-forbidden-external",
        "src/pkg/core/model.py:2:1",
        "pkg.core imports yaml, forbidden",
    ),
];

type Fault = Box<dyn std::error::Error>;

#[derive(Default)]
struct Said {
    check_out: String,
    check_err: String,
    check_code: i32,
    ext_out: String,
    ext_err: String,
    ext_code: i32,
}

fn capture(name: &str) -> String {
    fs::read_to_string(Path::new(CAPTURES).join(name)).unwrap_or_default()
}

fn replayed(variant: &str) -> Result<Said, Fault> {
    let exits = capture("exits.txt");
    let line = exits
        .lines()
        .find(|line| line.split(' ').next() == Some(variant))
        .ok_or("no such capture")?;
    let code = |key: &str| -> Result<i32, Fault> {
        let word = line
            .split(' ')
            .find_map(|part| part.strip_prefix(key))
            .ok_or("no exit")?;
        Ok(word.parse()?)
    };
    Ok(Said {
        check_out: capture(&format!("{variant}.check.out")),
        check_err: capture(&format!("{variant}.check.err")),
        check_code: code("check=")?,
        ext_out: capture(&format!("{variant}.ext.out")),
        ext_err: capture(&format!("{variant}.ext.err")),
        ext_code: code("external=")?,
    })
}

fn toml(extra: &str, ui: &str, core: &str, rules: &str) -> String {
    format!(
        "source_roots = [\"src\"]\n{LAYERS}{extra}\n[[modules]]\npath = \"pkg.ui\"\nlayer = \"ui\"\ndepends_on = [\"pkg.core\"]\n{ui}\n[[modules]]\npath = \"pkg.core\"\nlayer = \"core\"\ndepends_on = [\"pkg.low\"]\n{core}\n[[modules]]\npath = \"pkg.low\"\nlayer = \"low\"\ndepends_on = []\n\n{rules}"
    )
}

fn layout(project: &Project, variant: &str) -> Result<(), Fault> {
    for init in ["", "core/", "ui/", "low/"] {
        project.file(&format!("src/pkg/{init}__init__.py"), "")?;
    }
    let (config, view, model, extra) = match variant {
        "base" => (toml("", "", "", ""), "z = 1\n", "x = 1\n", None),
        "ok" => (toml("", "", "", ""), "from pkg.core import model\n", "x = 1\n", None),
        "undeclared" => (toml(STRICT, "", "", ""), "from pkg.low import base\n", "x = 1\n", None),
        "layer" => (toml("", "", "", ""), "z = 1\n", "import os\nfrom pkg.ui import view\n", None),
        "forbidden" => (
            toml(STRICT, "cannot_depend_on = [\"pkg.low\"]\n", "", ""),
            "from pkg.low import base\n",
            "x = 1\n",
            None,
        ),
        "visibility" => (
            toml("", "", "visibility = [\"pkg.low\"]\n", ""),
            "from pkg.core import model\n",
            "x = 1\n",
            None,
        ),
        "interfaces" => (
            format!(
                "{}[[interfaces]]\nexpose = [\"api\"]\nfrom = [\"pkg.core\"]\n",
                toml("", "", "", "")
            ),
            "from pkg.core.model import x\n",
            "x = 1\n",
            None,
        ),
        "ignores" => (
            toml(
                STRICT,
                "",
                "",
                "[rules]\nunused_ignore_directives = \"error\"\nrequire_ignore_directive_reasons = \"error\"\n",
            ),
            "from pkg.core import model  # tach-ignore\nfrom pkg.low import base  # tach-ignore(legacy) pkg.low.base\nimport os  # tach-ignore(why) pkg.core\n",
            "x = 1\n",
            None,
        ),
        "ext1" => (
            toml("", "", "", ""),
            "z = 1\n",
            "import requests\nimport yaml\n",
            Some(PYPROJECT),
        ),
        "modext" => (
            toml(
                "",
                "",
                "depends_on_external = [\"httpx\"]\ncannot_depend_on_external = [\"yaml\"]\n",
                "",
            ),
            "z = 1\n",
            "import requests\nimport yaml\nimport httpx\n",
            Some("[project]\nname = \"pkg\"\nversion = \"0\"\ndependencies = [\"httpx\", \"pyyaml\", \"requests\"]\n"),
        ),
        "unusedext" => (
            toml("", "", "", "[rules]\nunused_external_dependencies = \"warn\"\n"),
            "z = 1\n",
            "import yaml\n",
            Some(PYPROJECT),
        ),
        "unknownlayer" => (toml("", "", "", "").replace("layer = \"ui\"", "layer = \"nope\""), "from pkg.core import model\n", "x = 1\n", None),
        "cycle" => (
            "source_roots = [\"src\"]\nforbid_circular_dependencies = true\n\n[[modules]]\npath = \"pkg.core\"\ndepends_on = [\"pkg.low\"]\n\n[[modules]]\npath = \"pkg.low\"\ndepends_on = [\"pkg.core\"]\n".to_owned(),
            "z = 1\n",
            "from pkg.low import base\n",
            None,
        ),
        "broken" => ("source_roots = [\"src\"]\nlayers = [\n".to_owned(), "z = 1\n", "x = 1\n", None),
        "nocfg" => (String::new(), "z = 1\n", "x = 1\n", None),
        other => return Err(format!("no such variant: {other}").into()),
    };
    project.file("src/pkg/ui/view.py", view)?;
    project.file("src/pkg/core/model.py", model)?;
    project.file(
        "src/pkg/low/base.py",
        if variant == "cycle" {
            "from pkg.core import model\n"
        } else {
            "y = 1\n"
        },
    )?;
    if !config.is_empty() {
        project.file("tach.toml", &config)?;
    }
    extra.map(|text| project.file("pyproject.toml", text)).transpose()?;
    Ok(())
}

fn eager(variant: &str) -> Result<Project, Fault> {
    let project = Project::new()?;
    project.config(EAGER)?;
    layout(&project, variant)?;
    Ok(project)
}

fn stubbed(project: &Project, version: &str, said: &Said) -> Result<OsString, Fault> {
    let dir = project.path().join("stubs");
    let data = project.path().join("said");
    for (name, text) in [
        ("co", &said.check_out),
        ("ce", &said.check_err),
        ("xo", &said.ext_out),
        ("xe", &said.ext_err),
    ] {
        project.file(&format!("said/{name}"), text)?;
    }
    let script = format!(
        "echo \"$PWD|$*\" >> {log}\ncase \"$1\" in\n--version) echo 'tach {version}';;\ncheck) cat {data}/co; cat {data}/ce >&2; exit {cc};;\ncheck-external) cat {data}/xo; cat {data}/xe >&2; exit {xc};;\nesac",
        log = project.path().join("log").display(),
        data = data.display(),
        cc = said.check_code,
        xc = said.ext_code,
    );
    stub(&dir, TACH, &script)?;
    only(&[&dir, Path::new("/usr/bin"), Path::new("/bin")])
}

fn replay(project: &Project, variant: &str) -> Result<OsString, Fault> {
    stubbed(project, VERSION, &replayed(variant)?)
}

fn clean() -> Said {
    Said {
        check_out: CLEAN.to_owned(),
        ext_err: EXTERNAL_OK.to_owned(),
        ..Said::default()
    }
}

fn lines_of(out: &Out, rule: &str) -> Vec<String> {
    out.stdout
        .lines()
        .filter(|line| line.contains(&format!(": {rule} ")))
        .map(str::to_owned)
        .collect()
}

fn log(project: &Project) -> Vec<String> {
    fs::read_to_string(project.path().join("log"))
        .map(|text| text.lines().map(str::to_owned).collect())
        .unwrap_or_default()
}

fn calls(project: &Project) -> Vec<String> {
    let mut calls: Vec<String> = log(project)
        .iter()
        .filter_map(|line| line.split_once('|').map(|(_, args)| args.to_owned()))
        .collect();
    calls.sort();
    calls
}

fn custom(rules: &str) -> Result<Project, Fault> {
    let project = Project::with_rules("[\"src\"]", rules)?;
    Ok(project)
}

fn rule(id: &str, check: &str) -> String {
    format!("- id: {id}\n  message: m\n  check: {check}\n  fix: {{safety: none, reason: ported}}\n")
}

fn licence(rule: &str, path: &str, reach: &str) -> String {
    format!(
        "[[exception]]\nrule = \"{rule}\"\npath = \"{path}\"\nreach = \"{reach}\"\nreason = \"r\"\napproved = \"d\"\n"
    )
}

fn licensed(project: &Project, text: &str) -> Result<(), Fault> {
    project.config("roots = [\"src\"]\ncatalogues = [\"cat/\"]\nexceptions = \"ex.toml\"\n")?;
    project.file("ex.toml", text)?;
    Ok(())
}

fn expect(out: &Out, rule: &str, place: &str, rendered: &str, label: &str) {
    let lines = lines_of(out, rule);
    assert_eq!(lines.len(), 1, "{label}: {out:?}");
    assert!(
        lines[0].starts_with(&format!("{place}: {rule} ")),
        "{label}: {}",
        lines[0]
    );
    assert!(lines[0].ends_with(&format!(": {rendered}")), "{label}: {}", lines[0]);
    assert_eq!(out.code, 1, "{label}: {out:?}");
}

#[test]
fn tach_each_kind_replayed_from_the_captures_goes_to_its_rule() -> Outcome {
    for (variant, rule, place, rendered) in KINDS {
        let project = eager(variant)?;
        let path = replay(&project, variant)?;
        let out = project.check_on(&path, &["--select", rule])?;
        expect(&out, rule, place, rendered, variant);
    }
    Ok(())
}

#[test]
fn tach_each_kind_with_real_tach_goes_to_its_rule() -> Outcome {
    let Some(path) = path_with(&[TACH]) else {
        return skip(TACH);
    };
    for (variant, rule, place, rendered) in KINDS {
        let project = eager(variant)?;
        let out = project.check_on(&path, &["--select", rule])?;
        expect(&out, rule, place, rendered, variant);
    }
    Ok(())
}

#[test]
fn tach_every_one_of_the_twelve_rules_is_met_by_a_kind_of_the_table() -> Outcome {
    let listed = Project::new()?.run(&["rule", "--catalogue", "eager", "--output-format", "json"])?;
    let rules: Vec<Value> = serde_json::from_str(&listed.stdout)?;
    let mut tach: Vec<&str> = rules
        .iter()
        .filter(|rule| rule["check"]["engine"] == "tach")
        .filter_map(|rule| rule["id"].as_str())
        .collect();
    tach.sort_unstable();
    let mut met: Vec<&str> = KINDS.iter().map(|(_, rule, _, _)| *rule).collect();
    met.sort_unstable();
    assert_eq!(tach, met);
    assert_eq!(tach.len(), 12);
    Ok(())
}

#[test]
fn tach_clean_project_and_a_configuration_warning_pass() -> Outcome {
    let Some(path) = path_with(&[TACH]) else {
        return skip(TACH);
    };
    for variant in ["ok", "base"] {
        let project = eager(variant)?;
        let out = project.check_on(&path, &["--select", "natures.*"])?;
        assert!(passed(&out), "{variant}: {out:?}");
    }
    let replayed = eager("base")?;
    let path = replay(&replayed, "base")?;
    assert!(passed(&replayed.check_on(&path, &["--select", "natures.*"])?));
    Ok(())
}

#[test]
fn tach_no_configuration_file_is_a_fault_with_tachs_message_once_per_command() -> Outcome {
    const MESSAGE: &str = "tach: exited with 1: Configuration file not found. Run 'tach init' to get started!\n";
    let real = path_with(&[TACH]);
    for (label, project, path) in [
        ("replayed", eager("nocfg")?, None),
        ("real", eager("nocfg")?, real.clone()),
    ] {
        let path = match (path, label) {
            (Some(path), _) => path,
            (None, "real") => return skip(TACH),
            (None, _) => replay(&project, "nocfg")?,
        };
        for select in [
            &["--select", "natures.declared-dependency"][..],
            &["--select", "natures.*"][..],
        ] {
            let out = project.check_on(&path, select)?;
            assert_eq!(out.code, 2, "{label} {select:?}: {out:?}");
            assert_eq!(out.stdout, "1 fault: the run is incomplete.\n", "{label} {select:?}");
            assert_eq!(out.stderr, MESSAGE, "{label} {select:?}");
        }
    }
    Ok(())
}

#[test]
fn tach_broken_configuration_is_a_fault_with_the_parse_error() -> Outcome {
    for real in [false, true] {
        let project = eager("broken")?;
        let path = if real {
            let Some(path) = path_with(&[TACH]) else {
                return skip(TACH);
            };
            path
        } else {
            replay(&project, "broken")?
        };
        let out = project.check_on(&path, &["--select", "natures.layer-order"])?;
        assert_eq!(out.code, 2, "real {real}: {out:?}");
        assert_eq!(out.stdout, "1 fault: the run is incomplete.\n");
        assert!(
            out.stderr.starts_with(
                "tach: exited with 1: Failed to parse project config: TOML parse error at line 2, column 11"
            ),
            "real {real}: {}",
            out.stderr
        );
        assert!(out.stderr.contains("unclosed array, expected `]`"), "{}", out.stderr);
    }
    Ok(())
}

#[test]
fn tach_unknown_layer_is_a_configuration_error_fault_exit_2() -> Outcome {
    for real in [false, true] {
        let project = eager("unknownlayer")?;
        let path = if real {
            let Some(path) = path_with(&[TACH]) else {
                return skip(TACH);
            };
            path
        } else {
            replay(&project, "unknownlayer")?
        };
        let out = project.check_on(&path, &["--select", "natures.layer-order"])?;
        assert_eq!(out.code, 2, "real {real}: {out:?}");
        assert_eq!(out.stdout, "1 fault: the run is incomplete.\n");
        assert_eq!(out.stderr, "tach: UnknownLayer, layer nope\n", "real {real}");
    }
    Ok(())
}

#[test]
fn tach_missing_while_needed_exits_2_naming_the_tool_and_the_rule() -> Outcome {
    let project = eager("undeclared")?;
    let empty = project.path().join("nothing");
    fs::create_dir_all(&empty)?;
    let out = project.check_on(&only(&[&empty])?, &["--select", "natures.declared-dependency"])?;
    assert_eq!(out.code, 2, "{out:?}");
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr.starts_with(
            "tach: not found beside law, in .venv/bin nor on PATH; needed by `natures.declared-dependency`"
        ),
        "{}",
        out.stderr
    );
    Ok(())
}

#[test]
fn tach_not_needed_is_never_started_nor_looked_for() -> Outcome {
    let project = eager("undeclared")?;
    let empty = project.path().join("nothing");
    fs::create_dir_all(&empty)?;
    let out = project.check_on(&only(&[&empty])?, &["--select", "natures.logic-ports-only"])?;
    assert!(passed(&out), "{out:?}");
    Ok(())
}

#[test]
fn tach_cycle_beside_the_external_command_is_a_finding_and_no_fault() -> Outcome {
    for (variant, rule, total) in [
        ("cycle", "natures.no-cycle", 2),
        ("visibility", "natures.visible-dependency", 1),
    ] {
        let project = eager(variant)?;
        let path = replay(&project, variant)?;
        let out = project.check_on(&path, &["--select", "natures.*"])?;
        assert_eq!(out.code, 1, "{variant}: {out:?}");
        assert_eq!(lines_of(&out, rule).len(), 1, "{variant}: {}", out.stdout);
        assert_eq!(
            lines_of(&out, "natures.no-import-cycle").len(),
            total - 1,
            "{variant}: {}",
            out.stdout
        );
        assert!(
            out.stdout.contains(&format!(
                "Found {} ({}, 0 warnings).",
                noun(total, "finding", "findings"),
                noun(total, "error", "errors")
            )),
            "{variant}: {}",
            out.stdout
        );
        assert!(!out.stdout.contains("fault"), "{variant}: {}", out.stdout);
        assert_eq!(out.stderr, "", "{variant}");
    }
    Ok(())
}

#[test]
fn tach_cycle_with_real_tach_is_the_same_finding_and_no_fault() -> Outcome {
    let Some(path) = path_with(&[TACH]) else {
        return skip(TACH);
    };
    for (variant, rule) in [
        ("cycle", "natures.no-cycle"),
        ("visibility", "natures.visible-dependency"),
    ] {
        let project = eager(variant)?;
        let out = project.check_on(&path, &["--select", "natures.*"])?;
        assert_eq!(out.code, 1, "{variant}: {out:?}");
        assert_eq!(lines_of(&out, rule).len(), 1, "{variant}: {}", out.stdout);
        assert!(!out.stdout.contains("fault"), "{variant}: {}", out.stdout);
        assert_eq!(out.stderr, "", "{variant}");
    }
    Ok(())
}

fn doc(kind: &str, extra: &str) -> String {
    format!(
        "[{{\"Located\": {{\"file_path\": \"src/pkg/ui/view.py\", \"line_number\": 1, \"original_line_number\": 1, \"severity\": \"Error\"{extra}, \"details\": {{\"Code\": {{\"{kind}\": {{\"a\": \"b\"}}}}}}}}}}]"
    )
}

fn faulted(said: &Said, select: &str) -> Result<Out, Fault> {
    let project = eager("undeclared")?;
    let path = stubbed(&project, VERSION, said)?;
    project.check_on(&path, &["--select", select])
}

#[test]
fn tach_unknown_kind_in_a_document_is_a_fault_naming_it() -> Outcome {
    let said = Said {
        check_out: doc("BrandNewKind", ""),
        check_code: 1,
        ..Said::default()
    };
    let out = faulted(&said, "natures.declared-dependency")?;
    assert_eq!(out.code, 2, "{out:?}");
    assert_eq!(out.stdout, "1 fault: the run is incomplete.\n");
    assert_eq!(
        out.stderr,
        "tach: `BrandNewKind`: a kind of diagnostic e-law does not know\n"
    );
    let global = Said {
        check_out: "[{\"Global\": {\"severity\": \"Error\", \"details\": {\"Configuration\": {\"NewConfig\": []}}}}]"
            .to_owned(),
        check_code: 1,
        ..Said::default()
    };
    let out = faulted(&global, "natures.declared-dependency")?;
    assert_eq!(
        out.stderr,
        "tach: `NewConfig`: a kind of diagnostic e-law does not know\n"
    );
    assert_eq!(out.code, 2);
    Ok(())
}

#[test]
fn tach_document_with_unknown_extra_fields_is_still_read() -> Outcome {
    let said = Said {
        check_out: doc("UndeclaredDependency", ", \"added_by_a_newer_tach\": [1]"),
        check_code: 1,
        ..Said::default()
    };
    let out = faulted(&said, "natures.declared-dependency")?;
    assert_eq!(out.code, 1, "{out:?}");
    assert_eq!(out.stderr, "");
    assert_eq!(lines_of(&out, "natures.declared-dependency").len(), 1, "{}", out.stdout);
    Ok(())
}

#[test]
fn tach_unknown_failure_line_of_the_external_text_is_a_fault() -> Outcome {
    let said = Said {
        check_out: CLEAN.to_owned(),
        ext_err: "External Dependencies\n[FAIL] src/pkg/core/model.py:1: Something new.\n".to_owned(),
        ext_code: 1,
        ..Said::default()
    };
    let out = faulted(&said, "natures.declared-external")?;
    assert_eq!(out.code, 2, "{out:?}");
    assert_eq!(
        out.stderr,
        "tach: `[FAIL] src/pkg/core/model.py:1: Something new.`: a kind of diagnostic e-law does not know\n"
    );
    Ok(())
}

#[test]
fn tach_abnormal_exit_or_output_that_is_not_the_document_is_a_fault_not_zero_findings() -> Outcome {
    let cases = [
        (
            Said {
                check_out: "junk".to_owned(),
                check_code: 3,
                ..Said::default()
            },
            "natures.declared-dependency",
            "tach: exited with 3: junk\n",
        ),
        (
            Said {
                check_out: "not json".to_owned(),
                ..Said::default()
            },
            "natures.declared-dependency",
            "tach: exited with 0: not json\n",
        ),
        (
            Said {
                check_out: "{\"error\": \"Something else\"}".to_owned(),
                check_code: 1,
                ..Said::default()
            },
            "natures.declared-dependency",
            "tach: Something else\n",
        ),
        (
            Said {
                ext_err: "boom".to_owned(),
                ext_code: 2,
                ..Said::default()
            },
            "natures.declared-external",
            "tach: exited with 2: boom\n",
        ),
        (
            Said {
                ext_err: "weird".to_owned(),
                ext_code: 1,
                ..Said::default()
            },
            "natures.declared-external",
            "tach: exited with 1: weird\n",
        ),
    ];
    for (said, select, stderr) in cases {
        let out = faulted(&said, select)?;
        assert_eq!(out.code, 2, "{select}: {out:?}");
        assert_eq!(out.stdout, "1 fault: the run is incomplete.\n", "{select}");
        assert_eq!(out.stderr, stderr, "{select}");
    }
    Ok(())
}

#[test]
fn tach_place_the_file_does_not_have_is_a_fault() -> Outcome {
    let said = Said {
        check_out: doc("UndeclaredDependency", "").replace("\"line_number\": 1", "\"line_number\": 9"),
        check_code: 1,
        ..Said::default()
    };
    let out = faulted(&said, "natures.declared-dependency")?;
    assert_eq!(out.code, 2, "{out:?}");
    assert_eq!(
        out.stderr,
        "tach: src/pkg/ui/view.py:9:1: a place the file does not have\n"
    );
    Ok(())
}

#[test]
fn tach_rule_naming_an_unknown_kind_is_a_fault_before_tach_is_looked_for() -> Outcome {
    let project = custom(&rule("t.bad", "{engine: tach, code: Nope}"))?;
    project.file("src/a.py", "x = 1\n")?;
    let empty = project.path().join("nothing");
    fs::create_dir_all(&empty)?;
    let out = project.check_on(&only(&[&empty])?, &[])?;
    assert_eq!(out.code, 2, "{out:?}");
    assert_eq!(out.stderr, "tach: `Nope`: a kind of diagnostic e-law does not know\n");
    Ok(())
}

#[test]
fn tach_paths_of_the_run_filter_the_located_findings_and_never_the_project_ones() -> Outcome {
    let project = eager("layer")?;
    let path = replay(&project, "layer")?;
    let ui = project.check_on(&path, &["--select", "natures.layer-order", "src/pkg/ui"])?;
    assert!(passed(&ui), "{ui:?}");
    let core = project.check_on(&path, &["--select", "natures.layer-order", "src/pkg/core"])?;
    expect(
        &core,
        "natures.layer-order",
        "src/pkg/core/model.py:2:1",
        "pkg.core (core) imports pkg.ui.view (ui), a higher layer",
        "core",
    );
    let file = project.check_on(&path, &["--select", "natures.layer-order", "src/pkg/core/model.py"])?;
    assert_eq!(lines_of(&file, "natures.layer-order").len(), 1, "{file:?}");
    let cycle = eager("cycle")?;
    let path = replay(&cycle, "cycle")?;
    let elsewhere = cycle.check_on(&path, &["--select", "natures.no-cycle", "src/pkg/ui"])?;
    assert_eq!(lines_of(&elsewhere, "natures.no-cycle").len(), 1, "{elsewhere:?}");
    Ok(())
}

#[test]
fn tach_paths_filter_holds_with_real_tach_too() -> Outcome {
    let Some(path) = path_with(&[TACH]) else {
        return skip(TACH);
    };
    let project = eager("layer")?;
    let ui = project.check_on(&path, &["--select", "natures.layer-order", "src/pkg/ui"])?;
    assert!(passed(&ui), "{ui:?}");
    let all = project.check_on(&path, &["--select", "natures.layer-order"])?;
    assert_eq!(lines_of(&all, "natures.layer-order").len(), 1, "{all:?}");
    Ok(())
}

#[test]
fn tach_select_decides_which_of_the_two_commands_run() -> Outcome {
    let cases: [(&[&str], &[&str]); 5] = [
        (
            &["--select", "natures.declared-dependency"],
            &["--version", "check --output json"],
        ),
        (
            &["--select", "natures.declared-external"],
            &["--version", "check-external"],
        ),
        (
            &[
                "--select",
                "natures.declared-dependency",
                "--select",
                "natures.no-unused-external",
            ],
            &["--version", "check --output json", "check-external"],
        ),
        (
            &["--select", "natures.*"],
            &["--version", "check --output json", "check-external"],
        ),
        (&["--select", "natures.logic-ports-only"], &[]),
    ];
    for (select, expected) in cases {
        let project = eager("ok")?;
        let path = stubbed(&project, VERSION, &clean())?;
        let out = project.check_on(&path, select)?;
        assert!(passed(&out), "{select:?}: {out:?}");
        let mut expected = expected.to_vec();
        expected.sort_unstable();
        assert_eq!(calls(&project), expected, "{select:?}");
    }
    Ok(())
}

#[test]
fn tach_a_rule_naming_no_kind_runs_both_commands() -> Outcome {
    let project = custom(&rule("t.any", "{engine: tach}"))?;
    project.file("src/a.py", "x = 1\n")?;
    let path = stubbed(&project, VERSION, &clean())?;
    let out = project.check_on(&path, &[])?;
    assert!(passed(&out), "{out:?}");
    assert_eq!(calls(&project), ["--version", "check --output json", "check-external"]);
    Ok(())
}

#[test]
fn tach_runs_once_per_run_in_the_project_root_even_from_a_subdirectory() -> Outcome {
    let project = eager("ok")?;
    let path = stubbed(&project, VERSION, &clean())?;
    let out = project.run_on_in("src/pkg", &path, &["check", "--select", "natures.declared-dependency"])?;
    assert!(passed(&out), "{out:?}");
    let root = fs::canonicalize(project.path())?.display().to_string();
    let lines = log(&project);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(
        lines.iter().all(|line| line.starts_with(&format!("{root}|"))),
        "{lines:?}"
    );
    Ok(())
}

#[test]
fn tach_report_stamps_the_tool_version_only_when_a_tach_rule_runs() -> Outcome {
    let project = eager("ok")?;
    let path = stubbed(&project, "9.9.9", &clean())?;
    let out = project.check_on(&path, &["--select", "natures.*", "--output-format", "json"])?;
    let report: Value = serde_json::from_str(&out.stdout)?;
    assert_eq!(
        report["run"]["tools"],
        serde_json::json!([{"name": "tach", "version": "9.9.9"}])
    );
    let none = project.check_on(
        &path,
        &["--select", "natures.logic-ports-only", "--output-format", "json"],
    )?;
    let report: Value = serde_json::from_str(&none.stdout)?;
    assert!(report["run"]["tools"].is_null(), "{}", report["run"]);
    Ok(())
}

#[test]
fn tach_report_stamps_the_real_version() -> Outcome {
    let Some(path) = path_with(&[TACH]) else {
        return skip(TACH);
    };
    let project = eager("ok")?;
    let out = project.check_on(&path, &["--select", "natures.*", "--output-format", "json"])?;
    let report: Value = serde_json::from_str(&out.stdout)?;
    assert_eq!(
        report["run"]["tools"],
        serde_json::json!([{"name": "tach", "version": VERSION}])
    );
    Ok(())
}

#[test]
fn tach_warning_of_tach_is_a_finding_of_severity_warning_whatever_the_rule_says() -> Outcome {
    let rule = "natures.no-unused-external";
    let rendered = "httpx is declared in pkg, never imported";
    let replayed_project = eager("unusedext")?;
    let replayed_path = replay(&replayed_project, "unusedext")?;
    let mut runs = vec![("replayed", replayed_project, replayed_path)];
    if let Some(path) = path_with(&[TACH]) {
        runs.push(("real", eager("unusedext")?, path));
    }
    for (label, project, path) in runs {
        let out = project.check_on(&path, &["--select", rule])?;
        let lines = lines_of(&out, rule);
        assert_eq!(lines.len(), 1, "{label}: {out:?}");
        assert!(
            lines[0].starts_with(&format!("tach.toml:1:1: {rule} ")),
            "{label}: {}",
            lines[0]
        );
        assert!(lines[0].ends_with(&format!(": {rendered}")), "{label}: {}", lines[0]);
        assert!(
            out.stdout.ends_with("Found 1 finding (0 errors, 1 warning).\n"),
            "{label}: {}",
            out.stdout
        );
        assert_eq!(out.code, 0, "{label}: {out:?}");
        let json = project.check_on(&path, &["--select", rule, "--output-format", "json"])?;
        let report: Value = serde_json::from_str(&json.stdout)?;
        assert_eq!(report["findings"][0]["severity"], "warning", "{label}: {report}");
        assert_eq!(json.code, 0, "{label}");
    }
    Ok(())
}

#[test]
fn tach_unused_ignore_seen_by_both_commands_is_reported_once_at_the_real_line() -> Outcome {
    let select = [
        "--select",
        "natures.used-ignore",
        "--select",
        "natures.ignore-reason",
        "--select",
        "natures.declared-external",
    ];
    let replayed_project = eager("ignores")?;
    let replayed_path = replay(&replayed_project, "ignores")?;
    let mut runs = vec![("replayed", replayed_project, replayed_path)];
    if let Some(path) = path_with(&[TACH]) {
        runs.push(("real", eager("ignores")?, path));
    }
    for (label, project, path) in runs {
        let out = project.check_on(&path, &select)?;
        let unused = lines_of(&out, "natures.used-ignore");
        assert_eq!(unused.len(), 1, "{label}: {}", out.stdout);
        assert!(
            unused[0].starts_with("src/pkg/ui/view.py:3:1: "),
            "{label}: {}",
            unused[0]
        );
        let reasonless = lines_of(&out, "natures.ignore-reason");
        assert_eq!(reasonless.len(), 1, "{label}: {}", out.stdout);
        assert_eq!(out.code, 1, "{label}: {out:?}");
    }
    Ok(())
}

const TACH_PAIR: &str = "- id: t.b\n  message: b\n  check: {engine: tach, code: LayerViolation}\n  fix: {safety: none, reason: x}\n- id: t.a\n  message: a\n  check: {engine: tach, code: UndeclaredDependency}\n  fix: {safety: none, reason: x}\n- id: t.elif\n  message: elif\n  check: {engine: pattern, rule: {kind: elif_clause}}\n  fix: {safety: none, reason: x}\n";

fn pragma_project(comment: &str) -> Result<(Project, OsString), Fault> {
    let project = custom(TACH_PAIR)?;
    project.file("src/pkg/a.py", &format!("x = 1  {comment}\n"))?;
    let path = stubbed(&project, VERSION, &clean())?;
    Ok((project, path))
}

#[test]
fn tach_unlicensed_ignore_is_a_pragma_finding_naming_the_first_tach_rule_sorted() -> Outcome {
    for comment in [
        "# tach-ignore",
        "#tach-ignore(reason)",
        "# tach-ignore(r) a.b c.d",
        "# tach-ignore a.b",
        "# why # tach-ignore a.b",
    ] {
        let (project, path) = pragma_project(comment)?;
        let out = project.check_on(&path, &[])?;
        assert_eq!(
            out.stdout,
            format!(
                "src/pkg/a.py:1:8: {PRAGMA}: t.a has no registered exception\n\nFound 1 finding (1 error, 0 warnings).\n"
            ),
            "{comment}"
        );
        assert_eq!((out.code, out.stderr.as_str()), (1, ""), "{comment}");
    }
    Ok(())
}

#[test]
fn tach_comment_tach_does_not_read_as_a_directive_is_no_pragma() -> Outcome {
    for comment in [
        "# tach-ignore a.b, c",
        "# tach-ignore: a.b",
        "# tach-ignorex",
        "# note tach-ignore",
        "# tach-ignore(r)a.b",
        "# tach-ignore a.b # more",
    ] {
        let (project, path) = pragma_project(comment)?;
        let out = project.check_on(&path, &[])?;
        assert!(passed(&out), "{comment}: {out:?}");
    }
    Ok(())
}

#[test]
fn tach_ignore_is_licensed_by_the_licence_of_any_tach_rule_at_its_path() -> Outcome {
    for rule in ["t.a", "t.b"] {
        let (project, path) = pragma_project("# tach-ignore a.b")?;
        licensed(&project, &licence(rule, "src/pkg/a.py", "pragma"))?;
        let out = project.check_on(&path, &[])?;
        assert!(passed(&out), "{rule}: {out:?}");
    }
    let (project, path) = pragma_project("# tach-ignore a.b")?;
    licensed(&project, &licence("t.*", "src/pkg/*.py", "pragma"))?;
    assert!(passed(&project.check_on(&path, &[])?));
    Ok(())
}

#[test]
fn tach_ignore_is_not_licensed_by_another_path_another_reach_or_another_rule() -> Outcome {
    for (rule, path_of, reach) in [
        ("t.a", "src/pkg/b.py", "pragma"),
        ("t.a", "src/pkg/a.py", "file"),
        ("t.elif", "src/pkg/a.py", "pragma"),
    ] {
        let (project, path) = pragma_project("# tach-ignore")?;
        licensed(&project, &licence(rule, path_of, reach))?;
        let out = project.check_on(&path, &[])?;
        assert!(
            out.stdout
                .contains(&format!("{PRAGMA}: t.a has no registered exception")),
            "{rule} {path_of} {reach}: {}",
            out.stdout
        );
        assert_eq!(out.code, 1, "{rule} {path_of} {reach}");
    }
    Ok(())
}

#[test]
fn tach_ignore_is_left_alone_when_no_tach_rule_is_loaded() -> Outcome {
    let project = custom(common::ELIF)?;
    project.file("src/pkg/a.py", "x = 1  # tach-ignore\ny = 2  # tach-ignore(r) a.b\n")?;
    let out = project.check(&[])?;
    assert!(passed(&out), "{out:?}");
    Ok(())
}

#[test]
fn tach_ignore_with_a_tach_rule_loaded_but_none_selected_is_still_a_pragma() -> Outcome {
    let (project, path) = pragma_project("# tach-ignore")?;
    let out = project.check_on(&path, &["--select", "t.elif"])?;
    assert!(
        out.stdout
            .contains(&format!("{PRAGMA}: t.a has no registered exception")),
        "{out:?}"
    );
    assert_eq!(calls(&project), Vec::<String>::new(), "tach is not started");
    Ok(())
}

#[test]
fn tach_ignore_against_real_tach_is_silent_to_tach_and_judged_by_law() -> Outcome {
    let Some(path) = path_with(&[TACH]) else {
        return skip(TACH);
    };
    let project = eager("undeclared")?;
    project.file("src/pkg/ui/view.py", "from pkg.low import base  # tach-ignore\n")?;
    let out = project.check_on(&path, &["--select", "natures.*"])?;
    assert_eq!(
        out.stdout,
        format!(
            "src/pkg/ui/view.py:1:27: {PRAGMA}: natures.declared-dependency has no registered exception\n\nFound 1 finding (1 error, 0 warnings).\n"
        )
    );
    assert_eq!(out.code, 1);
    project.config("roots = [\"src\"]\ncatalogues = [\"eager\"]\nexceptions = \"ex.toml\"\n")?;
    project.file(
        "ex.toml",
        &licence("natures.layer-order", "src/pkg/ui/view.py", "pragma"),
    )?;
    let ok = project.check_on(&path, &["--select", "natures.*"])?;
    assert!(passed(&ok), "{ok:?}");
    Ok(())
}

#[test]
fn tach_unused_and_reasonless_ignores_are_reported_by_tach_beside_law_pragmas() -> Outcome {
    let Some(path) = path_with(&[TACH]) else {
        return skip(TACH);
    };
    let project = eager("ignores")?;
    let out = project.check_on(&path, &["--select", "natures.*"])?;
    let pragmas: Vec<&str> = out.stdout.lines().filter(|line| line.contains(PRAGMA)).collect();
    assert_eq!(pragmas.len(), 3, "{}", out.stdout);
    assert!(!lines_of(&out, "natures.ignore-reason").is_empty(), "{}", out.stdout);
    assert!(!lines_of(&out, "natures.used-ignore").is_empty(), "{}", out.stdout);
    Ok(())
}

#[test]
fn tach_trial_lists_the_rule_untested_with_its_why_and_never_starts_tach() -> Outcome {
    let rule = format!(
        "{}  cases:\n    invalid: [\"x = 1\\n\"]\n",
        rule("t.tach", "{engine: tach, code: LayerViolation}")
    );
    let project = custom(&rule)?;
    let empty = project.path().join("nothing");
    fs::create_dir_all(&empty)?;
    let out = project.run_on(&only(&[&empty])?, &["test"])?;
    assert_eq!(out.code, 0, "{out:?}");
    assert_eq!(
        out.stdout,
        "t.tach: untested: needs a project\n\nTested 0 rules, 0 cases: 0 failed, 1 untested.\n"
    );
    assert_eq!(out.stderr, "");
    Ok(())
}

#[test]
fn tach_every_captured_exit_is_what_the_sample_says() {
    let exits = capture("exits.txt");
    let table: Vec<(&str, &str)> = exits.lines().filter_map(|line| line.split_once(' ')).collect();
    let expected = [
        ("base", "check=0 external=0"),
        ("ok", "check=0 external=0"),
        ("undeclared", "check=1 external=0"),
        ("layer", "check=1 external=0"),
        ("forbidden", "check=1 external=0"),
        ("cycle", "check=1 external=1"),
        ("visibility", "check=1 external=1"),
        ("interfaces", "check=1 external=0"),
        ("ignores", "check=1 external=1"),
        ("nocfg", "check=1 external=1"),
        ("broken", "check=1 external=1"),
        ("unknownlayer", "check=1 external=0"),
        ("ext1", "check=0 external=1"),
        ("unusedext", "check=0 external=0"),
        ("modext", "check=0 external=1"),
    ];
    assert_eq!(table, expected);
    assert_eq!(capture("version.txt").trim(), format!("tach {VERSION}"));
}
