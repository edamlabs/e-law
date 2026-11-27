mod common;

use std::fs;
use std::path::PathBuf;

use common::{Outcome, Root, Said, TACH_RULES, fixture};
use law_engine::{BatchEngine, Sink};
use law_external::{Failure, TachEngine, Tool};
use law_model::RuleId;
use serde_json::Value;

const FILES: [(&str, &str); 3] = [
    ("src/pkg/ui/view.py", "from pkg.low import base\n"),
    ("src/pkg/core/model.py", "import os\nfrom pkg.ui import view\n"),
    ("tach.toml", "source_roots = [\"src\"]\n"),
];
const UNNAMED: &str = "- id: t.any\n  message: any\n  check: {engine: tach}\n  fix: {safety: none, reason: x}\n";
const MANIFEST: &str = "src/pkg/ui/view.py";
const UNKNOWN: &str = "[{\"Located\": {\"file_path\": \"src/pkg/ui/view.py\", \"line_number\": 1, \"original_line_number\": 1, \"severity\": \"Error\", \"details\": {\"Code\": {\"BrandNewKind\": {\"a\": \"b\"}}}}}]";

fn said(check_out: String) -> Said {
    Said {
        check_out,
        check_code: 1,
        ..Said::default()
    }
}

fn two() -> Result<String, Box<dyn std::error::Error>> {
    let mut both: Vec<Value> = Vec::new();
    for name in ["tach/undeclared.check.out", "tach/layer.check.out"] {
        both.extend(serde_json::from_str::<Vec<Value>>(&fixture(name)?)?);
    }
    Ok(serde_json::to_string(&both)?)
}

fn run(root: &Root, select: &[&str], paths: &[&str]) -> Result<Sink, Box<dyn std::error::Error>> {
    let engine = TachEngine::new(&root.loaded, &root.selection(select)?)
        .map_err(|fault| fault.to_string())?
        .ok_or("no engine")?;
    let mut sink = Sink::default();
    let paths: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    engine.check(root.dir.path(), &paths, &mut sink);
    Ok(sink)
}

fn faults(sink: &Sink) -> Vec<String> {
    sink.faults.iter().map(ToString::to_string).collect()
}

fn found(sink: &Sink) -> Vec<(String, String, u32, u32)> {
    let mut found: Vec<_> = sink
        .findings
        .iter()
        .map(|finding| {
            (
                finding.rule.as_str().to_owned(),
                finding.path.display().to_string(),
                finding.span.start.line,
                finding.span.start.column,
            )
        })
        .collect();
    found.sort();
    found
}

#[test]
fn tach_engine_is_none_when_no_selected_rule_is_a_tach_rule() -> Outcome {
    let root = Root::new(TACH_RULES, &Said::default(), &FILES)?;
    assert!(
        TachEngine::new(&root.loaded, &root.selection(&["t.elif"])?)
            .map_err(|f| f.to_string())?
            .is_none()
    );
    assert!(root.log().is_empty(), "tach was started: {:?}", root.log());
    Ok(())
}

#[test]
fn tach_engine_names_the_tool_its_version_and_the_selected_rules_in_catalogue_order() -> Outcome {
    let root = Root::new(TACH_RULES, &Said::default(), &FILES)?;
    let engine = TachEngine::new(&root.loaded, &root.selection(&["t.cycle", "t.layer"])?)
        .map_err(|f| f.to_string())?
        .ok_or("no engine")?;
    assert_eq!((engine.name(), engine.version()), ("tach", ""));
    engine.check(root.dir.path(), &[PathBuf::from(".")], &mut Sink::default());
    assert_eq!((engine.name(), engine.version()), ("tach", "9.9.9"));
    let rules: Vec<&str> = engine.rules().iter().map(RuleId::as_str).collect();
    assert_eq!(rules, ["t.layer", "t.cycle"]);
    Ok(())
}

#[test]
fn tach_engine_rule_naming_a_kind_e_law_does_not_know_is_a_fault_before_tach_runs() -> Outcome {
    let rules = "- id: t.bad\n  message: bad\n  check: {engine: tach, code: Nope}\n  fix: {safety: none, reason: x}\n";
    let root = Root::new(rules, &Said::default(), &FILES)?;
    let fault = TachEngine::new(&root.loaded, &root.selection(&["t.bad"])?)
        .err()
        .ok_or("built")?;
    assert_eq!(fault.tool, Tool::Tach);
    assert_eq!(
        fault.failure,
        Failure::Unknown {
            kind: "Nope".to_owned()
        }
    );
    assert!(root.log().is_empty(), "{:?}", root.log());
    Ok(())
}

#[test]
fn tach_engine_runs_one_command_by_the_kinds_selected() -> Outcome {
    let cases: [(&[&str], &str, &[&str]); 4] = [
        (
            &["t.layer", "t.undeclared", "t.cycle"],
            TACH_RULES,
            &["--version", "check --output json"],
        ),
        (&["t.external"], TACH_RULES, &["--version", "check-external"]),
        (
            &["t.layer", "t.external"],
            TACH_RULES,
            &["--version", "check --output json", "check-external"],
        ),
        (
            &["t.any"],
            UNNAMED,
            &["--version", "check --output json", "check-external"],
        ),
    ];
    for (select, rules, expected) in cases {
        let root = Root::new(rules, &Said::default(), &FILES)?;
        let sink = run(&root, select, &["."]);
        assert!(sink.is_err() || sink.is_ok(), "ran");
        let log = root.log();
        let canonical = fs::canonicalize(root.dir.path())?.display().to_string();
        assert!(
            log.iter().all(|line| line.starts_with(&format!("{canonical}|"))),
            "cwd is the root: {log:?}"
        );
        let mut args: Vec<&str> = log
            .iter()
            .filter_map(|line| line.split_once('|').map(|(_, args)| args))
            .collect();
        args.sort_unstable();
        let mut expected = expected.to_vec();
        expected.sort_unstable();
        assert_eq!(args, expected, "{select:?}");
    }
    Ok(())
}

#[test]
fn tach_engine_keeps_the_findings_within_the_paths_of_the_run() -> Outcome {
    let root = Root::new(TACH_RULES, &said(two()?), &FILES)?;
    let all = run(&root, &["t.undeclared", "t.layer"], &["."])?;
    assert_eq!(faults(&all), Vec::<String>::new());
    assert_eq!(
        found(&all),
        [
            ("t.layer".to_owned(), "src/pkg/core/model.py".to_owned(), 2, 1),
            ("t.undeclared".to_owned(), MANIFEST.to_owned(), 1, 1)
        ]
    );
    let ui = run(&root, &["t.undeclared", "t.layer"], &["src/pkg/ui"])?;
    assert_eq!(found(&ui), [("t.undeclared".to_owned(), MANIFEST.to_owned(), 1, 1)]);
    let none = run(&root, &["t.undeclared", "t.layer"], &["src/pkg/low"])?;
    assert!(none.findings.is_empty() && none.faults.is_empty());
    let file = run(
        &root,
        &["t.undeclared", "t.layer"],
        &[MANIFEST, "src/pkg/core/model.py"],
    )?;
    assert_eq!(found(&file).len(), 2);
    let prefix = run(&root, &["t.undeclared", "t.layer"], &["src/pkg/u"])?;
    assert!(
        prefix.findings.is_empty(),
        "a path is a prefix by component, not by text"
    );
    Ok(())
}

#[test]
fn tach_engine_message_is_the_rules_then_the_rendered_fields() -> Outcome {
    let root = Root::new(TACH_RULES, &said(two()?), &FILES)?;
    let sink = run(&root, &["t.undeclared"], &["."])?;
    assert_eq!(sink.findings.len(), 1);
    assert_eq!(
        sink.findings[0].message,
        "undeclared: pkg.ui imports pkg.low.base, undeclared"
    );
    assert!(sink.findings[0].fix.is_none());
    Ok(())
}

#[test]
fn tach_engine_project_level_findings_sit_at_tach_toml_whatever_the_paths() -> Outcome {
    let cycle = fixture("tach/cycle.check.out")?;
    let root = Root::new(TACH_RULES, &said(cycle), &FILES)?;
    for paths in [&["."][..], &["src/pkg/ui"], &["elsewhere"]] {
        let sink = run(&root, &["t.cycle"], paths)?;
        assert_eq!(
            found(&sink),
            [("t.cycle".to_owned(), "tach.toml".to_owned(), 1, 1)],
            "{paths:?}"
        );
        assert_eq!(
            sink.findings[0].message,
            "cycle: pkg.core, pkg.low depend on each other"
        );
    }
    Ok(())
}

#[test]
fn tach_engine_configuration_error_is_a_fault_and_a_warning_is_dropped() -> Outcome {
    let layer = Root::new(TACH_RULES, &said(fixture("tach/unknownlayer.check.out")?), &FILES)?;
    let sink = run(&layer, &["t.layer"], &["."])?;
    assert_eq!(faults(&sink), ["tach: UnknownLayer, layer nope"]);
    assert!(sink.findings.is_empty());
    let base = Root::new(
        TACH_RULES,
        &Said {
            check_out: fixture("tach/base.check.out")?,
            ..Said::default()
        },
        &FILES,
    )?;
    let sink = run(&base, &["t.layer"], &["."])?;
    assert!(sink.findings.is_empty() && sink.faults.is_empty());
    Ok(())
}

#[test]
fn tach_engine_unknown_kind_is_a_fault_naming_it_never_a_silent_drop() -> Outcome {
    let root = Root::new(TACH_RULES, &said(UNKNOWN.to_owned()), &FILES)?;
    let sink = run(&root, &["t.layer"], &["."])?;
    assert_eq!(
        faults(&sink),
        ["tach: `BrandNewKind`: a kind of diagnostic e-law does not know"]
    );
    assert!(sink.findings.is_empty());
    let outside = run(&root, &["t.layer"], &["src/pkg/core"])?;
    assert!(
        outside.faults.is_empty(),
        "a kind outside the paths of the run is not read"
    );
    Ok(())
}

#[test]
fn tach_engine_unknown_error_object_is_a_fault_with_tachs_own_word() -> Outcome {
    let root = Root::new(TACH_RULES, &said("{\"error\": \"Something else\"}".to_owned()), &FILES)?;
    let sink = run(&root, &["t.layer"], &["."])?;
    assert_eq!(faults(&sink), ["tach: Something else"]);
    Ok(())
}

#[test]
fn tach_engine_place_the_file_does_not_have_and_a_file_that_is_gone_are_faults() -> Outcome {
    let beyond = UNKNOWN
        .replace("\"line_number\": 1", "\"line_number\": 9")
        .replace("BrandNewKind", "UndeclaredDependency");
    let root = Root::new(TACH_RULES, &said(beyond), &FILES)?;
    let sink = run(&root, &["t.undeclared"], &["."])?;
    assert_eq!(
        faults(&sink),
        ["tach: src/pkg/ui/view.py:9:1: a place the file does not have"]
    );
    let gone = Root::new(TACH_RULES, &said(two()?), &[("tach.toml", "")])?;
    let sink = run(&gone, &["t.undeclared"], &["."])?;
    assert_eq!(sink.faults.len(), 2, "one per file: {:?}", faults(&sink));
    assert!(
        faults(&sink)
            .iter()
            .all(|fault| fault.contains(": No such file or directory")),
        "{:?}",
        faults(&sink)
    );
    Ok(())
}

#[test]
fn tach_engine_exit_other_than_0_or_1_or_unparseable_output_is_a_fault_with_tachs_words() -> Outcome {
    let cases = [
        (
            Said {
                check_out: "junk".to_owned(),
                check_code: 3,
                ..Said::default()
            },
            "tach: exited with 3: junk",
        ),
        (
            Said {
                check_out: "not json".to_owned(),
                check_code: 0,
                ..Said::default()
            },
            "tach: exited with 0: not json",
        ),
        (
            Said {
                check_out: "not json".to_owned(),
                check_code: 1,
                ..Said::default()
            },
            "tach: exited with 1: not json",
        ),
        (
            Said {
                check_err: "boom".to_owned(),
                check_code: 2,
                ..Said::default()
            },
            "tach: exited with 2: boom",
        ),
        (
            Said {
                check_out: "[]".to_owned(),
                check_code: 2,
                ..Said::default()
            },
            "tach: exited with 2: []",
        ),
    ];
    for (said, expected) in cases {
        let root = Root::new(TACH_RULES, &said, &FILES)?;
        let sink = run(&root, &["t.layer"], &["."])?;
        assert_eq!(faults(&sink), [expected]);
    }
    Ok(())
}

#[test]
fn tach_engine_external_text_exit_and_lines_decide_between_findings_and_faults() -> Outcome {
    let fail = fixture("tach/ext1.ext.err")?;
    let root = Root::new(
        TACH_RULES,
        &Said {
            ext_err: fail,
            ext_code: 1,
            ..Said::default()
        },
        &[
            ("src/pkg/core/model.py", "import requests\nimport yaml\n"),
            ("tach.toml", ""),
        ],
    )?;
    let sink = run(&root, &["t.external"], &["."])?;
    assert_eq!(faults(&sink), Vec::<String>::new());
    assert_eq!(
        found(&sink),
        [("t.external".to_owned(), "src/pkg/core/model.py".to_owned(), 1, 1)]
    );
    assert_eq!(
        sink.findings[0].message,
        "external: requests is imported, undeclared in pkg"
    );
    let weird = Root::new(
        TACH_RULES,
        &Said {
            ext_err: "weird".to_owned(),
            ext_code: 1,
            ..Said::default()
        },
        &FILES,
    )?;
    assert_eq!(
        faults(&run(&weird, &["t.external"], &["."])?),
        ["tach: exited with 1: weird"]
    );
    let unknown = Root::new(
        TACH_RULES,
        &Said {
            ext_err: "[FAIL] src/pkg/ui/view.py:1: Something new.\n".to_owned(),
            ext_code: 1,
            ..Said::default()
        },
        &FILES,
    )?;
    assert_eq!(
        faults(&run(&unknown, &["t.external"], &["."])?),
        ["tach: `[FAIL] src/pkg/ui/view.py:1: Something new.`: a kind of diagnostic e-law does not know"]
    );
    let three = Root::new(
        TACH_RULES,
        &Said {
            ext_err: "x".to_owned(),
            ext_code: 2,
            ..Said::default()
        },
        &FILES,
    )?;
    assert_eq!(
        faults(&run(&three, &["t.external"], &["."])?),
        ["tach: exited with 2: x"]
    );
    Ok(())
}
