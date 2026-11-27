mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use common::{Out, Outcome, Project, RUFF, TY, only, path_with, skip, stub};
use serde_json::Value;

const DIGEST: &str = "catalogues/eager";
const NATIVE: [&str; 6] = [
    "--select",
    "ost.private-lineage",
    "--select",
    "ost.privates-first",
    "--select",
    "ost.imports-at-top",
];

fn on_disk() -> String {
    format!("{}/../../{DIGEST}/", env!("CARGO_MANIFEST_DIR"))
}

fn rules(project: &Project, args: &[&str]) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let mut all = vec!["rule", "--output-format", "json"];
    all.extend_from_slice(args);
    let out = project.run(&all)?;
    assert_eq!((out.stderr.as_str(), out.code), ("", 0), "{out:?}");
    Ok(serde_json::from_str(&out.stdout)?)
}

fn eager(project: &Project) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    rules(project, &["--catalogue", "eager"])
}

fn engine_of(rule: &Value) -> &str {
    rule["check"]["engine"].as_str().unwrap_or_default()
}

fn digest(project: &Project, source: &str) -> Result<(String, String), Box<dyn std::error::Error>> {
    project.config(&format!("roots = [\"src\"]\ncatalogues = [\"{source}\"]\n"))?;
    project.file("src/pkg/a.py", "\"\"\"d\"\"\"\n")?;
    let report = project.json(&["--select", "ost.privates-first"])?;
    let stamp = &report["run"]["catalogues"][0];
    Ok((
        stamp["version"].as_str().ok_or("version")?.to_owned(),
        stamp["digest"].as_str().ok_or("digest")?.to_owned(),
    ))
}

fn stub_tools(project: &Project, stdout: &str) -> Result<std::ffi::OsString, Box<dyn std::error::Error>> {
    let dir = project.path().join("stubs");
    for tool in [RUFF, TY] {
        let version = if tool == RUFF { "0.17.6" } else { "0.0.112" };
        stub(
            &dir,
            tool,
            &format!("case \"$1\" in --version) echo \"{tool} {version}\";; *) cat <<'EOF'\n{stdout}\nEOF\n;; esac"),
        )?;
    }
    only(&[&dir, Path::new("/usr/bin"), Path::new("/bin")])
}

fn failing(out: &Out, id: &str) -> bool {
    out.stdout
        .lines()
        .any(|line| line.starts_with(&format!("{id}: invalid case")))
}

#[test]
fn builtin_eager_has_108_rules_by_engine() -> Outcome {
    let all = eager(&Project::new()?)?;
    let by: BTreeMap<&str, usize> = all.iter().fold(BTreeMap::new(), |mut by, rule| {
        *by.entry(engine_of(rule)).or_default() += 1;
        by
    });
    assert_eq!(all.len(), 108);
    assert_eq!(
        by,
        BTreeMap::from([
            ("graph", 2),
            ("members", 2),
            ("metric", 4),
            ("parity", 1),
            ("pattern", 45),
            ("profile", 8),
            ("ruff", 27),
            ("tach", 12),
            ("tree", 6),
            ("ty", 1)
        ])
    );
    Ok(())
}

#[test]
fn builtin_eager_profile_rules_follow_the_rulings() -> Outcome {
    let all = eager(&Project::new()?)?;
    let profiled: BTreeMap<&str, (&str, &str, Vec<&str>)> = all
        .iter()
        .filter(|rule| engine_of(rule) == "profile")
        .filter_map(|rule| {
            Some((
                rule["id"].as_str()?,
                (
                    rule["severity"].as_str()?,
                    rule["check"]["aspect"].as_str()?,
                    rule["check"]["kinds"]
                        .as_array()?
                        .iter()
                        .filter_map(Value::as_str)
                        .collect(),
                ),
            ))
        })
        .collect();
    assert_eq!(
        profiled,
        BTreeMap::from([
            ("lexis.no-abc-port", ("error", "never", vec!["port"])),
            (
                "natures.engine-present",
                ("error", "present", vec!["engine", "processor"])
            ),
            (
                "natures.engine-surname",
                ("warning", "surname", vec!["engine", "processor"])
            ),
            (
                "natures.provider-present",
                ("error", "present", vec!["driver", "adapter"])
            ),
            (
                "natures.provider-spec",
                ("error", "attributes", vec!["driver", "adapter"])
            ),
            (
                "natures.provider-surname",
                ("warning", "surname", vec!["driver", "adapter"])
            ),
            ("symbolon.port-protocol", ("error", "bases", vec!["port"])),
            (
                "symbolon.provider-lifecycle",
                ("error", "methods", vec!["driver", "adapter"])
            ),
        ])
    );
    assert!(all.iter().all(|rule| rule["id"] != "symbolon.port-suffix"));
    Ok(())
}

#[test]
fn builtin_eager_ids_are_unique_and_namespaced_by_a_law_set() -> Outcome {
    let all = eager(&Project::new()?)?;
    let ids: Vec<&str> = all.iter().filter_map(|rule| rule["id"].as_str()).collect();
    let unique: std::collections::BTreeSet<&str> = ids.iter().copied().collect();
    assert_eq!(ids.len(), unique.len());
    let spaces: std::collections::BTreeSet<&str> = ids.iter().filter_map(|id| id.split('.').next()).collect();
    assert_eq!(
        spaces,
        [
            "banks",
            "elenchos",
            "kairos",
            "lexis",
            "metron",
            "natures",
            "ost",
            "phronesis",
            "stratify",
            "symbolon"
        ]
        .into_iter()
        .collect()
    );
    Ok(())
}

#[test]
fn builtin_eager_ost_imports_at_top_is_the_renamed_rule_and_module_level_import_is_ruffs() -> Outcome {
    let project = Project::new()?;
    let all = eager(&project)?;
    let by = |id: &str| all.iter().find(|rule| rule["id"] == id);
    assert_eq!(by("ost.imports-at-top").map(engine_of), Some("pattern"));
    assert!(by("ost.late-import").is_none());
    let reserved = project.run(&["rule", "ost.module-level-import", "--catalogue", "eager"])?;
    assert!(
        reserved.stdout.contains("\nengine: ruff PLC0415\n"),
        "{}",
        reserved.stdout
    );
    Ok(())
}

#[test]
fn builtin_eager_members_rules_are_the_two_atomic_ost_rules() -> Outcome {
    let all = eager(&Project::new()?)?;
    let members: Vec<(&str, &Value)> = all
        .iter()
        .filter(|rule| engine_of(rule) == "members")
        .filter_map(|rule| Some((rule["id"].as_str()?, &rule["check"])))
        .collect();
    assert_eq!(members.len(), 2);
    let lineage = members
        .iter()
        .find(|(id, _)| *id == "ost.private-lineage")
        .ok_or("lineage")?
        .1;
    let order = members
        .iter()
        .find(|(id, _)| *id == "ost.privates-first")
        .ok_or("order")?
        .1;
    assert_eq!(lineage["lineage"]["common"], serde_json::json!(["common"]));
    assert!(lineage.get("order").is_none_or(Value::is_null));
    assert_eq!(order["order"], "privates-first");
    assert!(order.get("lineage").is_none_or(Value::is_null));
    let hooks = &lineage["hooks"];
    assert_eq!(hooks, &order["hooks"]);
    assert!(
        hooks
            .as_array()
            .is_some_and(|hooks| hooks.contains(&"field_validator".into())),
        "{hooks}"
    );
    Ok(())
}

#[test]
fn builtin_eager_symbolon_typed_is_the_one_ty_rule_with_no_rule_name() -> Outcome {
    let project = Project::new()?;
    let all = eager(&project)?;
    let typed: Vec<&Value> = all.iter().filter(|rule| engine_of(rule) == "ty").collect();
    assert_eq!(typed.len(), 1);
    assert_eq!(typed[0]["id"], "symbolon.typed");
    assert!(typed[0]["check"].get("rule").is_none_or(Value::is_null));
    let text = project.run(&["rule", "symbolon.typed", "--catalogue", "eager"])?;
    assert!(text.stdout.contains("\nengine: ty\n"), "{}", text.stdout);
    Ok(())
}

#[test]
fn builtin_eager_messages_never_prescribe_a_pragma_or_name_a_project() -> Outcome {
    let all = eager(&Project::new()?)?;
    for rule in &all {
        let message = rule["message"].as_str().unwrap_or_default();
        let id = rule["id"].as_str().unwrap_or_default();
        for banned in ["ast-grep-ignore", "aivo", "customer", "findr"] {
            assert!(!message.to_lowercase().contains(banned), "{id}: {message}");
        }
    }
    Ok(())
}

#[test]
fn builtin_eager_fixed_messages_say_register_an_exception() -> Outcome {
    let all = eager(&Project::new()?)?;
    for id in ["symbolon.no-object", "symbolon.no-cast", "banks.literalstring-cast"] {
        let rule = all.iter().find(|rule| rule["id"] == id).ok_or(id)?;
        let message = rule["message"].as_str().unwrap_or_default();
        assert!(message.contains("register an exception"), "{id}: {message}");
    }
    Ok(())
}

#[test]
fn builtin_eager_every_ruff_rule_is_a_single_plain_code_with_its_own_cases() -> Outcome {
    let all = eager(&Project::new()?)?;
    for rule in all.iter().filter(|rule| engine_of(rule) == "ruff") {
        let id = rule["id"].as_str().unwrap_or_default();
        let code = rule["check"]["code"].as_str().unwrap_or_default();
        let tailored = code == "TID251"
            && rule["check"]["message"].is_string()
            && rule["check"]["settings"]["lint.flake8-tidy-imports.banned-api"].is_object();
        assert!(!code.is_empty() && (code != "TID251" || tailored), "{id}: {code}");
        assert!(
            rule["cases"]["valid"].as_array().is_some_and(|cases| !cases.is_empty()),
            "{id}: no valid case"
        );
        assert!(
            rule["cases"]["invalid"]
                .as_array()
                .is_some_and(|cases| !cases.is_empty()),
            "{id}: no invalid case"
        );
    }
    Ok(())
}

#[test]
fn builtin_eager_proves_itself_with_no_project_and_the_real_tools() -> Outcome {
    let Some(path) = path_with(&[RUFF, TY]) else {
        return skip("ruff and ty");
    };
    let out = Project::new()?.run_on(&path, &["test", "--catalogue", "eager"])?;
    assert_eq!(
        out.stdout,
        "natures.declared-dependency: untested: needs a project\nnatures.layer-order: untested: needs a project\nnatures.no-forbidden-dependency: untested: needs a project\nnatures.no-cycle: untested: needs a project\nnatures.visible-dependency: untested: needs a project\nnatures.public-interface: untested: needs a project\nnatures.used-ignore: untested: needs a project\nnatures.ignore-reason: untested: needs a project\nnatures.declared-external: untested: needs a project\nnatures.no-unused-external: untested: needs a project\nnatures.declared-module-external: untested: needs a project\nnatures.no-forbidden-external: untested: needs a project\nnatures.provider-layout: untested: needs a directory\nost.logic-root: untested: needs a directory\nost.single-word-module: untested: needs a directory\nost.no-init: untested: needs a directory\nost.folder-vocabulary: untested: needs a directory\nost.no-toplevel-adapters: untested: needs a directory\nsymbolon.typed: untested: needs an environment\n\nTested 89 rules, 402 cases: 0 failed, 19 untested.\n"
    );
    assert_eq!((out.stderr.as_str(), out.code), ("", 0));
    Ok(())
}

#[test]
fn builtin_eager_trial_json_lists_the_untested_with_what_they_need() -> Outcome {
    let Some(path) = path_with(&[RUFF, TY]) else {
        return skip("ruff and ty");
    };
    let out = Project::new()?.run_on(&path, &["test", "--catalogue", "eager", "--output-format", "json"])?;
    let report: Value = serde_json::from_str(&out.stdout)?;
    assert_eq!(report["failed"], 0);
    assert_eq!(
        report["untested"],
        serde_json::json!([
            {"rule": "natures.declared-dependency", "need": "project"},
            {"rule": "natures.layer-order", "need": "project"},
            {"rule": "natures.no-forbidden-dependency", "need": "project"},
            {"rule": "natures.no-cycle", "need": "project"},
            {"rule": "natures.visible-dependency", "need": "project"},
            {"rule": "natures.public-interface", "need": "project"},
            {"rule": "natures.used-ignore", "need": "project"},
            {"rule": "natures.ignore-reason", "need": "project"},
            {"rule": "natures.declared-external", "need": "project"},
            {"rule": "natures.no-unused-external", "need": "project"},
            {"rule": "natures.declared-module-external", "need": "project"},
            {"rule": "natures.no-forbidden-external", "need": "project"},
            {"rule": "natures.provider-layout", "need": "directory"},
            {"rule": "ost.logic-root", "need": "directory"},
            {"rule": "ost.single-word-module", "need": "directory"},
            {"rule": "ost.no-init", "need": "directory"},
            {"rule": "ost.folder-vocabulary", "need": "directory"},
            {"rule": "ost.no-toplevel-adapters", "need": "directory"},
            {"rule": "symbolon.typed", "need": "environment"}
        ])
    );
    Ok(())
}

#[test]
fn builtin_eager_ruff_cases_really_run_through_ruff() -> Outcome {
    let project = Project::new()?;
    let path = stub_tools(&project, "[]")?;
    let out = project.run_on(&path, &["test", "--catalogue", "eager"])?;
    assert_eq!(out.code, 1, "{out:?}");
    let all = eager(&project)?;
    for rule in all.iter().filter(|rule| engine_of(rule) == "ruff") {
        let id = rule["id"].as_str().unwrap_or_default();
        assert!(
            failing(&out, id),
            "{id}: its invalid case passed with a ruff that finds nothing\n{}",
            out.stdout
        );
    }
    Ok(())
}

#[test]
fn builtin_eager_native_rules_pass_their_cases_with_no_external_tool() -> Outcome {
    let project = Project::new()?;
    let nothing = project.path().join("nothing");
    fs::create_dir_all(&nothing)?;
    let mut args = vec!["test", "--catalogue", "eager"];
    args.extend(NATIVE);
    let out = project.run_on(&only(&[&nothing])?, &args)?;
    assert!(
        out.stdout.starts_with("Tested 3 rules, ") && out.stdout.ends_with(" cases: 0 failed, 0 untested.\n"),
        "{out:?}"
    );
    assert_eq!(out.code, 0, "{out:?}");
    Ok(())
}

#[test]
fn builtin_eager_ruff_rules_without_ruff_on_the_path_are_a_fault_naming_the_tool() -> Outcome {
    let project = Project::new()?;
    let nothing = project.path().join("nothing");
    fs::create_dir_all(&nothing)?;
    let out = project.run_on(&only(&[&nothing])?, &["test", "--catalogue", "eager"])?;
    assert_eq!(out.code, 2, "{out:?}");
    assert!(out.stderr.contains("ruff"), "{}", out.stderr);
    Ok(())
}

#[test]
fn builtin_eager_selecting_only_members_rules_needs_no_tool_to_check() -> Outcome {
    let project = Project::new()?;
    project.config("roots = [\"src\"]\ncatalogues = [\"eager\"]\n")?;
    project.file("src/pkg/a.py", "class A:\n    def _stray(self): ...\n")?;
    let nothing = project.path().join("nothing");
    fs::create_dir_all(&nothing)?;
    let out = project.check_on(&only(&[&nothing])?, &["--select", "ost.private-lineage"])?;
    assert_eq!(out.code, 1, "{out:?}");
    assert!(out.stdout.contains("ost.private-lineage"), "{}", out.stdout);
    assert_eq!(out.stderr, "");
    Ok(())
}

#[test]
fn builtin_eager_a_coverage_pragma_is_one_finding_the_no_cover_rules() -> Outcome {
    let project = Project::new()?;
    project.config("roots = [\"src\"]\ncatalogues = [\"eager\"]\n")?;
    project.file("src/pkg/a.py", "x = 1  # pragma: no cover\n")?;
    let nothing = project.path().join("nothing");
    fs::create_dir_all(&nothing)?;
    let out = project.check_on(
        &only(&[&nothing])?,
        &["--select", "ost.comment-form", "--select", "elenchos.no-cover-pragma"],
    )?;
    assert_eq!(out.code, 1, "{out:?}");
    assert!(out.stdout.contains("elenchos.no-cover-pragma"), "{}", out.stdout);
    assert!(!out.stdout.contains("ost.comment-form"), "{}", out.stdout);
    assert_eq!(out.stdout.matches("src/pkg/a.py").count(), 1, "{}", out.stdout);
    Ok(())
}

#[test]
fn builtin_a_project_names_eager_with_no_directory_and_it_judges_its_code() -> Outcome {
    let project = Project::new()?;
    project.config("roots = [\"src\"]\ncatalogues = [\"eager\"]\n")?;
    project.file(
        "src/pkg/a.py",
        "class A:\n    def run(self): ...\n    def _late(self): ...\n",
    )?;
    let out = project.check(&["--select", "ost.private-lineage", "--select", "ost.privates-first"])?;
    assert_eq!(out.code, 1, "{out:?}");
    assert!(out.stdout.contains("ost.privates-first"), "{}", out.stdout);
    assert!(out.stdout.contains("ost.private-lineage"), "{}", out.stdout);
    Ok(())
}

#[test]
fn builtin_embedded_digest_equals_the_directorys() -> Outcome {
    let embedded = digest(&Project::new()?, "eager")?;
    let directory = digest(&Project::new()?, &on_disk())?;
    assert_eq!(embedded, directory);
    assert!(
        embedded.1.starts_with("sha256:") && embedded.1.len() == "sha256:".len() + 64,
        "{embedded:?}"
    );
    Ok(())
}

#[test]
fn builtin_embedded_rules_are_the_directorys_rules() -> Outcome {
    let project = Project::new()?;
    let embedded = eager(&project)?;
    let directory = rules(&project, &["--catalogue", &on_disk()])?;
    assert_eq!(embedded, directory);
    Ok(())
}

#[test]
fn builtin_flag_takes_a_directory_and_several_sources_in_order() -> Outcome {
    let project = Project::new()?;
    project.file(
        "mine/catalogue.toml",
        "name = \"mine\"\nversion = \"1\"\ndescription = \"d\"\n",
    )?;
    project.file(
        "mine/rules/r.yml",
        "- id: mine.one\n  message: m\n  check: {engine: pattern, rule: {kind: elif_clause}}\n  fix: {safety: none, reason: x}\n",
    )?;
    let both = rules(&project, &["--catalogue", "eager", "--catalogue", "mine/"])?;
    assert_eq!(both.len(), 109);
    assert_eq!(both.last().map(|rule| rule["id"].clone()), Some("mine.one".into()));
    let only_mine = rules(&project, &["--catalogue", "./mine"])?;
    assert_eq!(only_mine.len(), 1);
    Ok(())
}

#[test]
fn builtin_flag_replaces_the_projects_catalogues() -> Outcome {
    let project = Project::with_rules("[\"src\"]", common::ELIF)?;
    let own = rules(&project, &[])?;
    assert_eq!(own.len(), 1);
    let given = eager(&project)?;
    assert_eq!(given.len(), 108);
    assert!(given.iter().all(|rule| rule["id"] != "t.elif"));
    Ok(())
}

#[test]
fn builtin_flag_on_check_is_refused_by_the_grammar() -> Outcome {
    let out = Project::new()?.run(&["check", "--catalogue", "eager"])?;
    assert_eq!(out.code, 2);
    assert!(out.stderr.contains("--catalogue"), "{}", out.stderr);
    Ok(())
}

#[test]
fn builtin_unknown_name_or_missing_directory_through_the_flag_is_a_fault() -> Outcome {
    let project = Project::new()?;
    let unknown = project.run(&["rule", "--catalogue", "nowhere"])?;
    assert_eq!((unknown.stdout.as_str(), unknown.code), ("", 2));
    assert!(unknown.stderr.contains("nowhere"), "{}", unknown.stderr);
    let missing = project.run(&["test", "--catalogue", "./absent"])?;
    assert_eq!((missing.stdout.as_str(), missing.code), ("", 2));
    assert!(missing.stderr.contains("catalogue.toml"), "{}", missing.stderr);
    Ok(())
}

#[test]
fn builtin_rule_and_test_without_the_flag_or_a_project_say_there_is_no_law_toml() -> Outcome {
    let project = Project::new()?;
    for command in ["rule", "test"] {
        let out = project.run(&[command])?;
        assert_eq!(out.code, 2, "{command}");
        assert!(out.stderr.contains("no law.toml"), "{command}: {}", out.stderr);
    }
    Ok(())
}

#[test]
fn builtin_one_rule_of_eager_is_shown_with_its_cases() -> Outcome {
    let out = Project::new()?.run(&["rule", "ost.privates-first", "--catalogue", "eager"])?;
    assert_eq!(out.code, 0);
    assert!(out.stdout.starts_with("id: ost.privates-first\n"), "{}", out.stdout);
    assert!(out.stdout.contains("\nengine: members order\n"), "{}", out.stdout);
    assert!(
        out.stdout.contains("\nvalid:\n") && out.stdout.contains("\ninvalid:\n"),
        "{}",
        out.stdout
    );
    Ok(())
}
