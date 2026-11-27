mod common;

use common::{Out, Outcome, Project, found, noun, passed};
use serde_json::Value;

const ROOTS: &str = "[\"src\"]";
const SCOPE: &str = "  scope: {include: [\"src/*/*/state.py\"]}\n";
const STATE: &str = "src/pkg/engines/state.py";
const TWO: &str = "class EnginesState:\n    cache: int\n    lock: int\n";

fn rule(names: &str, entries: &str, relation: &str) -> String {
    format!(
        "- id: t.parity\n  message: mirror\n{SCOPE}  check: {{engine: parity, names: {{{names}}}, entries: {{{entries}}}, relation: {relation}}}\n  fix: {{safety: none, reason: x}}\n"
    )
}

fn attributes(entries: &str, relation: &str) -> String {
    rule("class: \"{Dir}State\", of: attributes", entries, relation)
}

fn project(rules: &str, files: &[(&str, &str)]) -> Result<Project, Box<dyn std::error::Error>> {
    let project = Project::with_rules(ROOTS, rules)?;
    for (path, code) in files {
        project.file(path, code)?;
    }
    Ok(project)
}

fn checked(rules: &str, files: &[(&str, &str)]) -> Result<Out, Box<dyn std::error::Error>> {
    project(rules, files)?.check(&[])
}

fn report(lines: &[&str]) -> String {
    format!(
        "{}\n\nFound {} ({}, 0 warnings).\n",
        lines.join("\n"),
        noun(lines.len(), "finding", "findings"),
        noun(lines.len(), "error", "errors"),
    )
}

fn refused(rules: &str) -> Result<Out, Box<dyn std::error::Error>> {
    project(rules, &[(STATE, TWO)])?.check(&[])
}

#[test]
fn parity_equal_attributes_and_modules_pass() -> Outcome {
    let out = checked(
        &attributes("beside: true", "equal"),
        &[
            (STATE, TWO),
            ("src/pkg/engines/cache.py", ""),
            ("src/pkg/engines/lock.py", ""),
        ],
    )?;
    assert!(passed(&out), "{out:?}");
    Ok(())
}

#[test]
fn parity_a_name_with_no_module_is_found_at_the_name() -> Outcome {
    let out = checked(
        &attributes("beside: true", "equal"),
        &[(STATE, TWO), ("src/pkg/engines/cache.py", "")],
    )?;
    assert_eq!(
        out.stdout,
        found(
            "src/pkg/engines/state.py:3:5",
            "t.parity",
            "mirror: lock names no module beside it",
            1,
            0
        )
    );
    assert_eq!((out.stderr.as_str(), out.code), ("", 1));
    Ok(())
}

#[test]
fn parity_a_module_with_no_name_is_found_at_the_class() -> Outcome {
    let out = checked(
        &attributes("beside: true", "equal"),
        &[
            (STATE, TWO),
            ("src/pkg/engines/cache.py", ""),
            ("src/pkg/engines/lock.py", ""),
            ("src/pkg/engines/queue.py", ""),
        ],
    )?;
    assert_eq!(
        out.stdout,
        found(
            "src/pkg/engines/state.py:1:7",
            "t.parity",
            "mirror: EnginesState has no attribute for queue",
            1,
            0
        )
    );
    Ok(())
}

#[test]
fn parity_each_relation_reports_its_own_side() -> Outcome {
    let files = [
        (STATE, TWO),
        ("src/pkg/engines/cache.py", ""),
        ("src/pkg/engines/queue.py", ""),
    ];
    let ghost = "src/pkg/engines/state.py:3:5: t.parity mirror: lock names no module beside it";
    let lost = "src/pkg/engines/state.py:1:7: t.parity mirror: EnginesState has no attribute for queue";
    for (relation, lines) in [
        ("equal", vec![lost, ghost]),
        ("names-in-entries", vec![ghost]),
        ("entries-in-names", vec![lost]),
    ] {
        let out = checked(&attributes("beside: true", relation), &files)?;
        assert_eq!(out.stdout, report(&lines), "{relation}");
    }
    Ok(())
}

#[test]
fn parity_exclude_leaves_entries_out_and_the_file_itself_is_never_one() -> Outcome {
    let files = [
        (STATE, "class EnginesState:\n    cache: int\n"),
        ("src/pkg/engines/cache.py", ""),
        ("src/pkg/engines/ports.py", ""),
        ("src/pkg/engines/models.py", ""),
        ("src/pkg/engines/__init__.py", ""),
    ];
    let out = checked(&attributes("beside: true", "equal"), &files)?;
    assert_eq!(
        out.stdout,
        report(&[
            "src/pkg/engines/state.py:1:7: t.parity mirror: EnginesState has no attribute for models",
            "src/pkg/engines/state.py:1:7: t.parity mirror: EnginesState has no attribute for ports",
        ])
    );
    let out = checked(&attributes("beside: true, exclude: [ports, models]", "equal"), &files)?;
    assert!(passed(&out), "{out:?}");
    Ok(())
}

#[test]
fn parity_a_package_beside_counts_and_an_empty_directory_does_not() -> Outcome {
    let project = project(
        &attributes("beside: true", "equal"),
        &[
            (STATE, "class EnginesState:\n    agents: int\n    deep: int\n"),
            ("src/pkg/engines/agents/ocr.py", ""),
            ("src/pkg/engines/deep/one/leaf.py", ""),
            ("src/pkg/engines/__pycache__/cache.cpython-314.pyc", ""),
        ],
    )?;
    project.dir("src/pkg/engines/empty")?;
    project.file("src/pkg/engines/assets/logo.png", "")?;
    let out = project.check(&[])?;
    assert!(passed(&out), "{out:?}");
    Ok(())
}

#[test]
fn parity_annotation_only_attributes_count_and_methods_nested_classes_and_dunders_do_not() -> Outcome {
    let state = "class EnginesState:\n    __slots__ = ()\n    cache: int\n    lock = None\n    class Inner:\n        ghost: int\n\n    def run(self) -> None: ...\n    @property\n    def name(self) -> str: ...\n";
    let out = checked(
        &attributes("beside: true", "equal"),
        &[
            (STATE, state),
            ("src/pkg/engines/cache.py", ""),
            ("src/pkg/engines/lock.py", ""),
        ],
    )?;
    assert!(passed(&out), "{out:?}");
    Ok(())
}

#[test]
fn parity_members_of_an_enum_are_lower_cased_valued_bindings() -> Outcome {
    let state =
        "class Kind(StrEnum):\n    VECTOR = auto()\n    HYBRID = \"hybrid\"\n    note: str\n    __ignore__ = 1\n";
    let rules = rule("class: \"Kind\", of: members", "beside: true", "equal");
    let scope = rules.replace("src/*/*/state.py", "src/*/*/kind.py");
    let out = checked(
        &scope,
        &[
            ("src/pkg/search/kind.py", state),
            ("src/pkg/search/vector.py", ""),
            ("src/pkg/search/hybrid.py", ""),
        ],
    )?;
    assert!(passed(&out), "{out:?}");
    let out = checked(
        &scope,
        &[("src/pkg/search/kind.py", state), ("src/pkg/search/vector.py", "")],
    )?;
    assert_eq!(
        out.stdout,
        found(
            "src/pkg/search/kind.py:3:5",
            "t.parity",
            "mirror: HYBRID names no module beside it",
            1,
            0
        )
    );
    Ok(())
}

#[test]
fn parity_members_name_the_member_in_the_missing_entry_message() -> Outcome {
    let rules = rule("class: \"Kind\", of: members", "beside: true", "entries-in-names")
        .replace("src/*/*/state.py", "src/*/*/kind.py");
    let out = checked(
        &rules,
        &[
            ("src/pkg/search/kind.py", "class Kind(StrEnum):\n    VECTOR = auto()\n"),
            ("src/pkg/search/vector.py", ""),
            ("src/pkg/search/lexical.py", ""),
        ],
    )?;
    assert_eq!(
        out.stdout,
        found(
            "src/pkg/search/kind.py:1:7",
            "t.parity",
            "mirror: Kind has no member for lexical",
            1,
            0
        )
    );
    Ok(())
}

#[test]
fn parity_a_glob_takes_the_stems_of_the_files_it_matches_whatever_their_extension() -> Outcome {
    let rules = rule(
        "class: \"{Dir}State\", of: attributes",
        "glob: \"src/*/agents/*.md\"",
        "equal",
    );
    let files = [
        (STATE, "class EnginesState:\n    ocr: int\n    summary: int\n"),
        ("src/pkg/agents/ocr.md", ""),
        ("src/pkg/agents/summary.md", ""),
        ("src/pkg/agents/notes.txt", ""),
        ("src/other/agents/elsewhere.md", ""),
    ];
    let out = checked(&rules, &files)?;
    assert_eq!(
        out.stdout,
        report(&["src/pkg/engines/state.py:1:7: t.parity mirror: EnginesState has no attribute for elsewhere"]),
        "{out:?}"
    );
    let rules = rules.replace("src/*/agents/*.md", "src/pkg/agents/*.md");
    let out = checked(&rules, &files)?;
    assert!(passed(&out), "{out:?}");
    Ok(())
}

#[test]
fn parity_a_glob_name_with_no_file_says_which_glob() -> Outcome {
    let rules = rule(
        "class: \"{Dir}State\", of: attributes",
        "glob: \"src/pkg/agents/*.md\"",
        "names-in-entries",
    );
    let out = checked(
        &rules,
        &[
            (STATE, "class EnginesState:\n    ocr: int\n"),
            ("src/pkg/agents/summary.md", ""),
        ],
    )?;
    assert_eq!(
        out.stdout,
        found(
            "src/pkg/engines/state.py:2:5",
            "t.parity",
            "mirror: ocr names no file of src/pkg/agents/*.md",
            1,
            0
        )
    );
    Ok(())
}

#[test]
fn parity_a_glob_whose_directory_does_not_exist_matches_nothing() -> Outcome {
    let rules = rule(
        "class: \"{Dir}State\", of: attributes",
        "glob: \"src/nowhere/*.md\"",
        "names-in-entries",
    );
    let out = checked(&rules, &[(STATE, "class EnginesState:\n    ocr: int\n")])?;
    assert_eq!(
        out.stdout,
        found(
            "src/pkg/engines/state.py:2:5",
            "t.parity",
            "mirror: ocr names no file of src/nowhere/*.md",
            1,
            0
        )
    );
    Ok(())
}

#[test]
fn parity_only_the_class_the_directory_names_is_judged_a_helper_state_is_not() -> Outcome {
    let state = "class AgentsState:\n    ghost: int\n\n\nclass EnginesState:\n    lock: int\n\n\nclass Helper:\n    other: int\n";
    let out = checked(
        &attributes("beside: true", "names-in-entries"),
        &[(STATE, state), ("src/pkg/engines/cache.py", "")],
    )?;
    assert_eq!(
        out.stdout,
        found(
            "src/pkg/engines/state.py:6:5",
            "t.parity",
            "mirror: lock names no module beside it",
            1,
            0
        )
    );
    Ok(())
}

#[test]
fn parity_the_template_fills_a_snake_directory_and_the_stem_in_pascal_case() -> Outcome {
    let rules = rule(
        "class: \"{Dir}{Stem}\", of: attributes",
        "beside: true",
        "names-in-entries",
    );
    let out = checked(
        &rules,
        &[
            ("src/pkg/my_engines/state.py", "class MyEnginesState:\n    ghost: int\n"),
            ("src/pkg/my_engines/cache.py", ""),
        ],
    )?;
    assert_eq!(
        out.stdout,
        found(
            "src/pkg/my_engines/state.py:2:5",
            "t.parity",
            "mirror: ghost names no module beside it",
            1,
            0
        )
    );
    Ok(())
}

#[test]
fn parity_a_template_with_no_placeholder_is_an_exact_class_name() -> Outcome {
    let rules = rule(
        "class: \"Registry\", of: attributes",
        "beside: true",
        "names-in-entries",
    );
    let out = checked(
        &rules,
        &[
            (
                STATE,
                "class Registry:\n    ghost: int\n\n\nclass RegistryState:\n    other: int\n",
            ),
            ("src/pkg/engines/cache.py", ""),
        ],
    )?;
    assert_eq!(
        out.stdout,
        found(
            "src/pkg/engines/state.py:2:5",
            "t.parity",
            "mirror: ghost names no module beside it",
            1,
            0
        )
    );
    Ok(())
}

#[test]
fn parity_a_file_in_scope_with_no_matching_class_is_found_at_its_first_line() -> Outcome {
    let out = checked(
        &attributes("beside: true", "equal"),
        &[
            (STATE, "class Helper:\n    cache: int\n"),
            ("src/pkg/engines/cache.py", ""),
        ],
    )?;
    assert_eq!(
        out.stdout,
        found(
            "src/pkg/engines/state.py:1:1",
            "t.parity",
            "mirror: no class named EnginesState",
            1,
            0
        )
    );
    Ok(())
}

#[test]
fn parity_the_class_named_is_matched_whole_and_among_top_level_classes_only() -> Outcome {
    let state = "class Wrapper:\n    class EnginesState:\n        ghost: int\n\n\nclass StateBase:\n    ghost: int\n";
    let out = checked(&attributes("beside: true", "equal"), &[(STATE, state)])?;
    assert_eq!(
        out.stdout,
        found(
            "src/pkg/engines/state.py:1:1",
            "t.parity",
            "mirror: no class named EnginesState",
            1,
            0
        )
    );
    Ok(())
}

#[test]
fn parity_only_the_files_in_scope_declare_names() -> Outcome {
    let out = checked(
        &attributes("beside: true", "equal"),
        &[
            ("src/pkg/engines/other.py", "class EnginesState:\n    ghost: int\n"),
            ("src/pkg/deep/er/state.py", "class EnginesState:\n    ghost: int\n"),
        ],
    )?;
    assert!(passed(&out), "{out:?}");
    Ok(())
}

#[test]
fn parity_findings_come_in_the_same_order_every_run() -> Outcome {
    let files = [
        (STATE, "class EnginesState:\n    zed: int\n    alpha: int\n"),
        ("src/pkg/engines/mid.py", ""),
        ("src/pkg/engines/beta.py", ""),
        ("src/pkg/engines/omega.py", ""),
    ];
    let project = project(&attributes("beside: true", "equal"), &files)?;
    let first = project.check(&[])?;
    for _ in 0..4 {
        assert_eq!(project.check(&[])?.stdout, first.stdout);
    }
    let lines: Vec<&str> = first.stdout.lines().take(5).collect();
    assert_eq!(
        lines,
        [
            "src/pkg/engines/state.py:1:7: t.parity mirror: EnginesState has no attribute for beta",
            "src/pkg/engines/state.py:1:7: t.parity mirror: EnginesState has no attribute for mid",
            "src/pkg/engines/state.py:1:7: t.parity mirror: EnginesState has no attribute for omega",
            "src/pkg/engines/state.py:2:5: t.parity mirror: zed names no module beside it",
            "src/pkg/engines/state.py:3:5: t.parity mirror: alpha names no module beside it",
        ]
    );
    Ok(())
}

#[test]
fn parity_a_path_given_still_lists_the_whole_directory() -> Outcome {
    let out = project(
        &attributes("beside: true", "equal"),
        &[
            (STATE, "class EnginesState:\n    cache: int\n"),
            ("src/pkg/engines/cache.py", ""),
        ],
    )?
    .check(&[STATE])?;
    assert!(passed(&out), "{out:?}");
    Ok(())
}

#[test]
fn parity_a_finding_is_json_with_its_rule_and_span() -> Outcome {
    let json = project(
        &attributes("beside: true", "equal"),
        &[(STATE, "class EnginesState:\n    cache: int\n")],
    )?
    .json(&[])?;
    let finding = &json["findings"][0];
    assert_eq!(finding["rule"], Value::from("t.parity"));
    assert_eq!(finding["span"]["start"]["line"], Value::from(2));
    Ok(())
}

#[test]
fn parity_load_refuses_a_class_template_that_does_not_compile() -> Outcome {
    let out = refused(&rule("class: \"[\", of: attributes", "beside: true", "equal"))?;
    assert_eq!((out.stdout.as_str(), out.code), ("", 2), "{out:?}");
    assert!(out.stderr.contains("bad-template"), "{}", out.stderr);
    Ok(())
}

#[test]
fn parity_load_refuses_a_class_name_that_is_a_glob_an_unknown_placeholder_or_empty() -> Outcome {
    for class in ["*State", "{Dir}?", "{Dir}[A-Z]", "{Nope}State", "{Dir}{A,B}", ""] {
        let out = refused(&rule(
            &format!("class: \"{class}\", of: attributes"),
            "beside: true",
            "equal",
        ))?;
        assert_eq!((out.stdout.as_str(), out.code), ("", 2), "{class}: {out:?}");
        assert!(out.stderr.contains("bad-template"), "{class}: {}", out.stderr);
    }
    Ok(())
}

#[test]
fn parity_a_class_name_is_exact_a_glob_lookalike_among_the_classes_is_not_judged() -> Outcome {
    let state = "class EnginesState:\n    cache: int\n\n\nclass OtherState:\n    ghost: int\n";
    let out = checked(
        &attributes("beside: true", "equal"),
        &[(STATE, state), ("src/pkg/engines/cache.py", "")],
    )?;
    assert!(passed(&out), "{out:?}");
    Ok(())
}

#[test]
fn parity_load_refuses_entries_that_name_neither_or_both_sources() -> Outcome {
    for entries in [
        "exclude: [state]",
        "beside: false",
        "beside: true, glob: \"src/*.md\"",
        "beside: false, exclude: []",
    ] {
        let out = refused(&attributes(entries, "equal"))?;
        assert_eq!((out.stdout.as_str(), out.code), ("", 2), "{entries}: {out:?}");
        assert!(out.stderr.contains("bad-entries"), "{entries}: {}", out.stderr);
    }
    Ok(())
}

#[test]
fn parity_load_refuses_a_glob_that_does_not_compile() -> Outcome {
    let out = refused(&attributes("glob: \"src/[\"", "equal"))?;
    assert_eq!((out.stdout.as_str(), out.code), ("", 2), "{out:?}");
    assert!(out.stderr.contains("bad-glob"), "{}", out.stderr);
    Ok(())
}

#[test]
fn parity_load_refuses_an_unknown_word_a_missing_field_and_an_unknown_key() -> Outcome {
    let good = attributes("beside: true", "equal");
    for (name, bad, needle) in [
        (
            "of",
            good.replace("of: attributes", "of: fields"),
            "unknown variant `fields`",
        ),
        (
            "relation",
            good.replace("relation: equal", "relation: same"),
            "unknown variant `same`",
        ),
        (
            "no relation",
            good.replace(", relation: equal", ""),
            "missing field `relation`",
        ),
        (
            "no names",
            good.replace("names: {class: \"{Dir}State\", of: attributes}, ", ""),
            "missing field `names`",
        ),
        (
            "no entries",
            good.replace(", entries: {beside: true}", ""),
            "missing field `entries`",
        ),
        (
            "key",
            good.replace("relation: equal", "relation: equal, depth: 2"),
            "unknown field `depth`",
        ),
        (
            "names key",
            good.replace("of: attributes", "of: attributes, deep: 1"),
            "unknown field `deep`",
        ),
        (
            "entries key",
            good.replace("beside: true", "beside: true, deep: 1"),
            "unknown field `deep`",
        ),
    ] {
        let out = refused(&bad)?;
        assert_eq!((out.stdout.as_str(), out.code), ("", 2), "{name}: {out:?}");
        assert!(out.stderr.contains(needle), "{name}: {}", out.stderr);
    }
    Ok(())
}

#[test]
fn parity_rule_text_names_what_is_compared_and_how() -> Outcome {
    for (entries, relation, engine) in [
        ("beside: true", "equal", "parity attributes of {Dir}State beside, equal"),
        (
            "beside: true, exclude: [state]",
            "names-in-entries",
            "parity attributes of {Dir}State beside, names-in-entries",
        ),
        (
            "glob: \"src/*/agents/*.md\"",
            "entries-in-names",
            "parity attributes of {Dir}State glob src/*/agents/*.md, entries-in-names",
        ),
    ] {
        let out = project(&attributes(entries, relation), &[])?.run(&["rule", "t.parity"])?;
        assert_eq!((out.stderr.as_str(), out.code), ("", 0), "{entries}");
        assert_eq!(
            out.stdout.lines().nth(4),
            Some(format!("engine: {engine}").as_str()),
            "{}",
            out.stdout
        );
    }
    Ok(())
}

#[test]
fn parity_rule_json_keeps_its_spec() -> Outcome {
    let out = project(&attributes("beside: true, exclude: [state]", "equal"), &[])?.run(&[
        "rule",
        "t.parity",
        "--output-format",
        "json",
    ])?;
    let rule: Value = serde_json::from_str(&out.stdout)?;
    assert_eq!(
        rule["check"],
        serde_json::json!({
            "engine": "parity",
            "names": {"class": "{Dir}State", "of": "attributes"},
            "entries": {"beside": true, "exclude": ["state"]},
            "relation": "equal",
        })
    );
    Ok(())
}

fn with_cases(cases: &str) -> String {
    format!("{}  cases:\n{cases}", attributes("beside: true", "equal"))
}

const WHOLE: &str = "      - files:\n          src/pkg/engines/state.py: \"class EnginesState:\\n    cache: int\\n\"\n          src/pkg/engines/cache.py: \"\"\n";
const BROKEN: &str = "      - files:\n          src/pkg/engines/state.py: \"class EnginesState:\\n    cache: int\\n    lock: int\\n\"\n          src/pkg/engines/cache.py: \"\"\n";

#[test]
fn parity_case_files_judge_the_declaring_file_with_its_neighbours() -> Outcome {
    let out = Project::with_rules(
        ROOTS,
        &with_cases(&format!("    valid:\n{WHOLE}    invalid:\n{BROKEN}")),
    )?
    .run(&["test"])?;
    assert_eq!(
        (out.stdout.as_str(), out.stderr.as_str(), out.code),
        ("Tested 1 rule, 2 cases: 0 failed, 0 untested.\n", "", 0)
    );
    Ok(())
}

#[test]
fn parity_case_files_a_valid_that_finds_and_an_invalid_that_does_not_fail() -> Outcome {
    let out = Project::with_rules(
        ROOTS,
        &with_cases(&format!("    valid:\n{BROKEN}    invalid:\n{WHOLE}")),
    )?
    .run(&["test"])?;
    assert_eq!(
        out.stdout,
        "t.parity: valid case 1: expected no finding, found 1 (the first at 3:5)\nt.parity: invalid case 1: expected a finding, found none\n\nTested 1 rule, 2 cases: 2 failed, 0 untested.\n"
    );
    assert_eq!(out.code, 1);
    Ok(())
}

#[test]
fn parity_case_files_see_only_their_own_files_never_the_disk() -> Outcome {
    let project = Project::with_rules(
        ROOTS,
        &with_cases(&format!("    valid:\n{WHOLE}    invalid:\n{BROKEN}")),
    )?;
    project.file("src/pkg/engines/queue.py", "")?;
    let out = project.run(&["test"])?;
    assert_eq!(out.stdout, "Tested 1 rule, 2 cases: 0 failed, 0 untested.\n", "{out:?}");
    Ok(())
}

#[test]
fn parity_case_code_is_a_load_fault() -> Outcome {
    for cases in [
        "    valid: [\"x = 1\\n\"]\n",
        "    invalid:\n      - {path: src/pkg/engines/state.py, code: \"class EnginesState: ...\\n\"}\n",
    ] {
        for command in ["test", "check"] {
            let out = Project::with_rules(ROOTS, &with_cases(cases))?.run(&[command])?;
            assert_eq!(
                (out.stdout.as_str(), out.stderr.as_str(), out.code),
                (
                    "",
                    "cat/rules/r.yml: rule `t.parity` runs on `parity`, a parity rule is proven by `files` cases, not a `code` case\n",
                    2
                ),
                "{cases} / {command}"
            );
        }
    }
    Ok(())
}
