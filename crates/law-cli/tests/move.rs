mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;

use common::{Out, Outcome, Project};

type Fault = Box<dyn std::error::Error>;

const MODULE: &str = "src/pkg/engines/cache.py";
const MOVED: &str = "src/pkg/engines/cache/engine.py";
const BODY: &str = "class CacheEngine: ...\n\n\nclass Row: ...\n";
const PROFILE: &str =
    "- kind: engine\n  home: [\"src/*/engines/**/*.py\"]\n  suffix: Engine\n  thresholds: {lines: 0}\n";
const RULE: &str = "- id: t.move
  message: over
  scope: {exclude: [\"**/__init__.py\"]}
  severity: warning
  check: {engine: metric, kinds: [engine], measure: lines, over: lines, relocate: package}
  fix: {safety: unsafe}
  cases:
    invalid:
      - path: src/pkg/engines/cache.py
        code: \"class CacheEngine: ...\\n\"
    fixed:
      - files: {src/pkg/engines/cache.py: \"class CacheEngine: ...\\n\"}
        expect: {src/pkg/engines/cache/engine.py: \"class CacheEngine: ...\\n\"}
";
const BOTH: [&str; 2] = ["--fix", "--unsafe-fixes"];

fn world(files: &[(&str, &str)]) -> Result<Project, Fault> {
    let project = Project::with_rules("[\"src\", \"tests\"]", RULE)?;
    project.file("cat/profiles/p.yml", PROFILE)?;
    project.file(MODULE, BODY)?;
    project.dir("tests")?;
    for (path, text) in files {
        project.file(path, text)?;
    }
    Ok(project)
}

fn text(project: &Project, relative: &str) -> Result<String, Fault> {
    Ok(fs::read_to_string(project.path().join(relative))?)
}

fn rewrites(before: &str, after: &str) -> Outcome {
    let project = world(&[("src/pkg/app.py", before)])?;
    let out = project.check(&BOTH)?;
    assert_eq!(out.code, 0, "{out:?}");
    assert_eq!(text(&project, "src/pkg/app.py")?, after, "{out:?}");
    assert_eq!(text(&project, MOVED)?, BODY);
    assert!(!project.path().join(MODULE).exists());
    Ok(())
}

fn untouched(project: &Project, files: &[(&str, &str)]) -> Outcome {
    for (path, expected) in files {
        assert_eq!(text(project, path)?, *expected, "{path}");
    }
    assert!(project.path().join(MODULE).exists());
    assert!(!project.path().join(MOVED).exists());
    Ok(())
}

///// every import form /////

#[test]
fn move_import_of_the_module_and_its_attribute_chains() -> Outcome {
    rewrites(
        "import os\nimport pkg.engines.cache\n\nx = pkg.engines.cache.CacheEngine()\ny = pkg.engines.cache\n",
        "import os\nimport pkg.engines.cache.engine\n\nx = pkg.engines.cache.engine.CacheEngine()\ny = pkg.engines.cache.engine\n",
    )
}

#[test]
fn move_import_among_several_names_of_one_statement() -> Outcome {
    rewrites(
        "import os, pkg.engines.cache, sys\n",
        "import os, pkg.engines.cache.engine, sys\n",
    )
}

#[test]
fn move_import_as_an_alias_keeps_the_alias_and_its_uses() -> Outcome {
    rewrites(
        "import pkg.engines.cache as c\n\nx = c.CacheEngine()\n",
        "import pkg.engines.cache.engine as c\n\nx = c.CacheEngine()\n",
    )
}

#[test]
fn move_from_import_of_the_module_keeps_every_name() -> Outcome {
    rewrites(
        "from pkg.engines.cache import CacheEngine, Row\nfrom pkg.engines.cache import *\n",
        "from pkg.engines.cache.engine import CacheEngine, Row\nfrom pkg.engines.cache.engine import *\n",
    )
}

#[test]
fn move_from_import_of_the_module_as_a_name() -> Outcome {
    rewrites(
        "from pkg.engines import cache\n\nx = cache.CacheEngine()\n",
        "from pkg.engines.cache import engine as cache\n\nx = cache.CacheEngine()\n",
    )
}

#[test]
fn move_from_import_of_the_module_as_a_name_with_an_alias() -> Outcome {
    rewrites(
        "from pkg.engines import cache as k\n",
        "from pkg.engines.cache import engine as k\n",
    )
}

#[test]
fn move_from_import_mixed_splits_the_statement_and_keeps_the_indentation() -> Outcome {
    rewrites(
        "from pkg.engines import cache, Thing\n\nif x:\n    from pkg.engines import Other, cache as k, More\n",
        "from pkg.engines import Thing\nfrom pkg.engines.cache import engine as cache\n\nif x:\n    from pkg.engines import Other, More\n    from pkg.engines.cache import engine as k\n",
    )
}

#[test]
fn move_from_import_mixed_in_parentheses_over_several_lines() -> Outcome {
    rewrites(
        "from pkg.engines import (\n    Thing,\n    cache,\n    More,\n)\n",
        "from pkg.engines import (\n    Thing,\n    More,\n)\nfrom pkg.engines.cache import engine as cache\n",
    )
}

#[test]
fn move_from_import_mixed_on_a_one_line_block_is_joined_by_a_semicolon() -> Outcome {
    rewrites(
        "if x: from pkg.engines import Thing, cache\n",
        "if x: from pkg.engines import Thing; from pkg.engines.cache import engine as cache\n",
    )
}

#[test]
fn move_type_checking_imports_and_string_annotations() -> Outcome {
    rewrites(
        "from typing import TYPE_CHECKING\n\nif TYPE_CHECKING:\n    import pkg.engines.cache\n    from pkg.engines.cache import CacheEngine\n\n\ndef f(a: \"pkg.engines.cache.CacheEngine\", b: 'list[pkg.engines.cache.Row] | None') -> \"pkg.engines.cache.Row\":\n    note = \"pkg.engines.cache.CacheEngine\"\n    return b\n",
        "from typing import TYPE_CHECKING\n\nif TYPE_CHECKING:\n    import pkg.engines.cache.engine\n    from pkg.engines.cache.engine import CacheEngine\n\n\ndef f(a: \"pkg.engines.cache.engine.CacheEngine\", b: 'list[pkg.engines.cache.engine.Row] | None') -> \"pkg.engines.cache.engine.Row\":\n    note = \"pkg.engines.cache.CacheEngine\"\n    return b\n",
    )
}

#[test]
fn move_crlf_files_split_with_crlf() -> Outcome {
    rewrites(
        "from pkg.engines import cache, Thing\r\n",
        "from pkg.engines import Thing\r\nfrom pkg.engines.cache import engine as cache\r\n",
    )
}

#[test]
fn move_names_that_only_look_alike_are_left() -> Outcome {
    let same = "import pkg.engines.cachier\nfrom pkg.engines.cache2 import X\nfrom other.engines import cache\n\nx = pkg.engines.cachier.Y\nz = a.pkg.engines.cache.W\n";
    let project = world(&[("src/pkg/app.py", same)])?;
    let out = project.check(&BOTH)?;
    assert_eq!(out.code, 0, "{out:?}");
    assert_eq!(text(&project, "src/pkg/app.py")?, same);
    assert_eq!(text(&project, MOVED)?, BODY);
    Ok(())
}

#[test]
fn move_the_importers_in_every_root_are_rewritten_tests_too() -> Outcome {
    let project = world(&[
        ("src/pkg/app.py", "from pkg.engines.cache import CacheEngine\n"),
        (
            "tests/test_cache.py",
            "import pkg.engines.cache as cache\nfrom pkg.engines import cache as other\n",
        ),
        ("tests/unit/test_deep.py", "from pkg.engines.cache import Row\n"),
    ])?;
    let out = project.check(&BOTH)?;
    assert_eq!(out.code, 0, "{out:?}");
    assert_eq!(
        text(&project, "src/pkg/app.py")?,
        "from pkg.engines.cache.engine import CacheEngine\n"
    );
    assert_eq!(
        text(&project, "tests/test_cache.py")?,
        "import pkg.engines.cache.engine as cache\nfrom pkg.engines.cache import engine as other\n"
    );
    assert_eq!(
        text(&project, "tests/unit/test_deep.py")?,
        "from pkg.engines.cache.engine import Row\n"
    );
    Ok(())
}

#[test]
fn move_a_package_init_that_re_exports_the_module_follows() -> Outcome {
    let project = world(&[(
        "src/pkg/engines/__init__.py",
        "from pkg.engines.cache import CacheEngine\n",
    )])?;
    let out = project.check(&BOTH)?;
    assert_eq!(out.code, 0, "{out:?}");
    assert_eq!(
        text(&project, "src/pkg/engines/__init__.py")?,
        "from pkg.engines.cache.engine import CacheEngine\n"
    );
    Ok(())
}

#[test]
fn move_the_project_root_spelling_of_a_module_is_rewritten_as_written() -> Outcome {
    let project = Project::with_rules("[\"src\", \".\"]", RULE)?;
    project.file("cat/profiles/p.yml", PROFILE)?;
    project.file(MODULE, BODY)?;
    project.file("src/pkg/app.py", "from src.pkg.engines.cache import CacheEngine\n")?;
    let out = project.check(&BOTH)?;
    assert_eq!(out.code, 0, "{out:?}");
    assert_eq!(
        text(&project, "src/pkg/app.py")?,
        "from src.pkg.engines.cache.engine import CacheEngine\n"
    );
    Ok(())
}

///// refusals and faults: nothing is written /////

fn blocked(out: &Out, why: &str) {
    assert_eq!(out.code, 2, "{out:?}");
    assert!(out.stderr.contains(why), "{}", out.stderr);
}

#[test]
fn move_a_relative_import_of_the_module_is_a_fault_and_nothing_is_written() -> Outcome {
    let importer = "from .engines import cache\n";
    let project = world(&[
        ("src/pkg/app.py", importer),
        ("src/pkg/other.py", "from pkg.engines.cache import Row\n"),
    ])?;
    let out = project.check(&BOTH)?;
    blocked(&out, "src/pkg/app.py");
    assert!(out.stderr.contains("relative import"), "{}", out.stderr);
    untouched(
        &project,
        &[
            ("src/pkg/app.py", importer),
            ("src/pkg/other.py", "from pkg.engines.cache import Row\n"),
        ],
    )
}

#[test]
fn move_a_relative_import_of_something_else_does_not_stop_it() -> Outcome {
    let project = world(&[
        (
            "src/pkg/app.py",
            "from . import other\nfrom pkg.engines.cache import Row\n",
        ),
        ("src/pkg/other.py", "x = 1\n"),
    ])?;
    let out = project.check(&BOTH)?;
    assert_eq!(out.code, 0, "{out:?}");
    assert_eq!(
        text(&project, "src/pkg/app.py")?,
        "from . import other\nfrom pkg.engines.cache.engine import Row\n"
    );
    Ok(())
}

#[test]
fn move_a_relative_import_inside_the_module_itself_is_a_fault_and_nothing_is_written() -> Outcome {
    let project = Project::with_rules("[\"src\", \"tests\"]", RULE)?;
    project.file("cat/profiles/p.yml", PROFILE)?;
    project.file(MODULE, "from . import other\n\nclass CacheEngine: ...\n")?;
    project.file("src/pkg/app.py", "import pkg.engines.cache\n")?;
    let out = project.check(&BOTH)?;
    blocked(&out, MODULE);
    assert!(project.path().join(MODULE).exists() && !project.path().join(MOVED).exists());
    assert_eq!(text(&project, "src/pkg/app.py")?, "import pkg.engines.cache\n");
    Ok(())
}

#[test]
fn move_a_relative_import_is_a_fault_only_when_the_fix_is_asked() -> Outcome {
    let project = world(&[("src/pkg/app.py", "from .engines import cache\n")])?;
    let out = project.check(&[])?;
    assert_eq!(out.code, 0, "{out:?}");
    assert_eq!(out.stderr, "");
    let safe = project.check(&["--fix"])?;
    assert_eq!(safe.code, 0, "{safe:?}");
    assert_eq!(safe.stderr, "");
    Ok(())
}

#[test]
fn move_onto_a_file_that_exists_is_a_fault_and_nothing_is_written() -> Outcome {
    let importer = "from pkg.engines.cache import Row\n";
    let project = world(&[("src/pkg/app.py", importer), (MOVED, "mine = 1\n")])?;
    let out = project.check(&BOTH)?;
    blocked(&out, "a move onto a file that exists");
    assert_eq!(text(&project, "src/pkg/app.py")?, importer);
    assert_eq!(text(&project, MOVED)?, "mine = 1\n");
    assert_eq!(text(&project, MODULE)?, BODY);
    Ok(())
}

#[test]
fn move_of_a_read_only_module_is_a_fault_and_nothing_is_written() -> Outcome {
    let importer = "from pkg.engines.cache import Row\n";
    let project = world(&[("src/pkg/app.py", importer)])?;
    fs::set_permissions(project.path().join(MODULE), fs::Permissions::from_mode(0o444))?;
    let out = project.check(&BOTH)?;
    blocked(&out, "read-only");
    untouched(&project, &[("src/pkg/app.py", importer)])
}

#[test]
fn move_an_importer_that_is_read_only_is_a_fault_and_nothing_is_written() -> Outcome {
    let importer = "from pkg.engines.cache import Row\n";
    let project = world(&[("src/pkg/app.py", importer)])?;
    fs::set_permissions(project.path().join("src/pkg/app.py"), fs::Permissions::from_mode(0o444))?;
    let out = project.check(&BOTH)?;
    blocked(&out, "read-only");
    untouched(&project, &[("src/pkg/app.py", importer)])
}

///// the flags, and what a second check says /////

#[test]
fn move_fix_alone_leaves_the_tree_untouched() -> Outcome {
    let importer = "from pkg.engines.cache import Row\n";
    let project = world(&[("src/pkg/app.py", importer)])?;
    let out = project.check(&["--fix"])?;
    assert_eq!(out.code, 0, "{out:?}");
    assert!(
        out.stdout
            .contains("src/pkg/engines/cache.py:1:1: t.move over: 2 lines, over 0"),
        "{}",
        out.stdout
    );
    assert!(!out.stdout.contains("Fixed"), "{}", out.stdout);
    untouched(&project, &[("src/pkg/app.py", importer)])
}

#[test]
fn move_the_fix_is_in_the_report_edits_first_the_move_last() -> Outcome {
    let project = world(&[("src/pkg/app.py", "from pkg.engines.cache import Row\n")])?;
    let report = project.json(&[])?;
    let fix = &report["findings"][0]["fix"];
    assert_eq!(fix["safety"], "unsafe");
    let edits = fix["edits"].as_array().ok_or("no edits")?;
    assert_eq!(edits.len(), 2, "{fix}");
    assert_eq!(edits[0]["edit"], "replace");
    assert_eq!(edits[0]["path"], "src/pkg/app.py");
    assert_eq!(
        edits[1],
        serde_json::json!({"edit": "move", "from": MODULE, "to": MOVED})
    );
    Ok(())
}

#[test]
fn move_the_second_check_finds_the_same_measure_at_the_new_path_with_no_fix() -> Outcome {
    let project = world(&[("src/pkg/app.py", "from pkg.engines.cache import Row\n")])?;
    let first = project.check(&BOTH)?;
    assert_eq!(first.code, 0, "{first:?}");
    assert!(first.stdout.contains("Fixed 1 finding."), "{}", first.stdout);
    let again = project.json(&[])?;
    let findings = again["findings"].as_array().ok_or("no findings")?;
    assert_eq!(findings.len(), 1, "{again}");
    assert_eq!(findings[0]["path"], MOVED);
    assert_eq!(findings[0]["rule"], "t.move");
    assert!(findings[0]["fix"].is_null(), "{}", findings[0]);
    let third = project.check(&BOTH)?;
    assert!(!third.stdout.contains("Fixed"), "{}", third.stdout);
    assert_eq!(text(&project, MOVED)?, BODY);
    Ok(())
}

#[test]
fn move_a_module_that_is_already_a_kind_file_gets_no_fix() -> Outcome {
    let project = Project::with_rules("[\"src\"]", RULE)?;
    project.file("cat/profiles/p.yml", PROFILE)?;
    project.file(MOVED, BODY)?;
    let report = project.json(&[])?;
    assert!(report["findings"][0]["fix"].is_null(), "{report}");
    Ok(())
}

#[test]
fn move_two_modules_in_one_import_line_land_over_two_passes() -> Outcome {
    let project = world(&[
        ("src/pkg/engines/store.py", "class StoreEngine: ...\n"),
        ("src/pkg/app.py", "from pkg.engines import cache, store\n"),
    ])?;
    let out = project.check(&BOTH)?;
    assert_eq!(out.code, 0, "{out:?}");
    let mut lines: Vec<String> = text(&project, "src/pkg/app.py")?.lines().map(str::to_owned).collect();
    lines.sort();
    assert_eq!(
        lines,
        [
            "from pkg.engines.cache import engine as cache",
            "from pkg.engines.store import engine as store"
        ]
    );
    assert!(project.path().join("src/pkg/engines/store/engine.py").exists());
    assert!(project.path().join(MOVED).exists());
    assert!(!project.path().join(MODULE).exists() && !project.path().join("src/pkg/engines/store.py").exists());
    Ok(())
}

#[test]
fn move_a_path_outside_the_run_is_not_moved() -> Outcome {
    let project = world(&[
        ("src/pkg/other.py", "x = 1\n"),
        ("src/pkg/engines/store.py", "class StoreEngine: ...\n"),
    ])?;
    let out = project.check(&["--fix", "--unsafe-fixes", "src/pkg/engines/store.py"])?;
    assert_eq!(out.code, 0, "{out:?}");
    assert!(project.path().join(MODULE).exists());
    assert!(project.path().join("src/pkg/engines/store/engine.py").exists());
    Ok(())
}

#[test]
fn move_a_moved_module_that_imports_another_moved_module_is_rewritten_then_moved() -> Outcome {
    let project = world(&[("src/pkg/engines/store.py", "class StoreEngine: ...\n")])?;
    project.file(
        MODULE,
        "from pkg.engines.store import StoreEngine\n\n\nclass CacheEngine(StoreEngine): ...\n",
    )?;
    let out = project.check(&BOTH)?;
    assert_eq!(out.code, 0, "{out:?}");
    assert_eq!(
        text(&project, MOVED)?,
        "from pkg.engines.store.engine import StoreEngine\n\n\nclass CacheEngine(StoreEngine): ...\n"
    );
    assert!(project.path().join("src/pkg/engines/store/engine.py").exists());
    assert!(!project.path().join(MODULE).exists() && !project.path().join("src/pkg/engines/store.py").exists());
    Ok(())
}

#[test]
fn move_a_rule_that_moves_needs_a_fix_policy_and_a_fix_policy_needs_the_move() -> Outcome {
    for rule in [
        RULE.replace("fix: {safety: unsafe}", "fix: {safety: none, reason: x}"),
        RULE.replace(", relocate: package", ""),
    ] {
        let project = Project::with_rules("[\"src\"]", &rule)?;
        project.file("cat/profiles/p.yml", PROFILE)?;
        project.file(MODULE, BODY)?;
        let out = project.check(&[])?;
        assert_eq!(out.code, 2, "{out:?}");
        assert!(out.stderr.contains("t.move"), "{}", out.stderr);
    }
    Ok(())
}

///// law test proves an unsafe fix across files /////

#[test]
fn move_law_test_proves_a_fix_across_files() -> Outcome {
    let project = world(&[])?;
    let out = project.run(&["test"])?;
    assert_eq!(out.code, 0, "{out:?}");
    Ok(())
}

#[test]
fn move_law_test_fails_a_wrong_expectation() -> Outcome {
    let wrong = RULE.replace("cache/engine.py: ", "cache/driver.py: ");
    assert_ne!(wrong, RULE);
    let project = Project::with_rules("[\"src\"]", &wrong)?;
    project.file("cat/profiles/p.yml", PROFILE)?;
    let out = project.run(&["test"])?;
    assert_eq!(out.code, 1, "{out:?}");
    assert!(out.stdout.contains("the fix gives"), "{}", out.stdout);
    Ok(())
}
