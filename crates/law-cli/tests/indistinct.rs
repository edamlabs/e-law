mod common;

use common::{Outcome, Project, only, passed, stub};
use std::fs;
use std::path::Path;

fn rule(id: &str, check: &str) -> String {
    format!("- id: {id}\n  message: m\n  check: {check}\n  fix: {{safety: none, reason: ported}}\n")
}

fn load(rules: &str) -> Result<common::Out, Box<dyn std::error::Error>> {
    let project = Project::with_rules("[\"pkg\"]", rules)?;
    project.file("pkg/a.py", "x = 1\n")?;
    let bin = project.path().join("bin");
    stub(
        &bin,
        "ruff",
        "case \"$1\" in --version) echo 'ruff 9.9.9';; *) echo '[]';; esac",
    )?;
    stub(
        &bin,
        "ty",
        "case \"$1\" in --version) echo 'ty 9.9.9';; *) echo '{\"runs\":[{\"results\":[]}]}';; esac",
    )?;
    project.check_on(&only(&[&bin, Path::new("/usr/bin"), Path::new("/bin")])?, &[])
}

#[test]
fn indistinct_two_ruff_rules_of_one_code_without_a_message_is_a_load_fault() -> Outcome {
    let rules = format!(
        "{}{}",
        rule("t.one", "{engine: ruff, code: T201}"),
        rule("t.two", "{engine: ruff, code: T201}")
    );
    let out = load(&rules)?;
    assert_eq!(out.code, 2, "{out:?}");
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr
            .contains("rules `t.one` and `t.two` both take every ruff `T201` diagnostic: nothing tells them apart"),
        "{}",
        out.stderr
    );
    Ok(())
}

#[test]
fn indistinct_ruff_one_code_split_by_message_loads() -> Outcome {
    for rules in [
        format!(
            "{}{}",
            rule("t.one", "{engine: ruff, code: TID251, message: environ}"),
            rule("t.two", "{engine: ruff, code: TID251}")
        ),
        format!(
            "{}{}",
            rule("t.one", "{engine: ruff, code: TID251, message: environ}"),
            rule("t.two", "{engine: ruff, code: TID251, message: skip}")
        ),
        format!(
            "{}{}",
            rule("t.one", "{engine: ruff, code: T201}"),
            rule("t.two", "{engine: ruff, code: E722}")
        ),
    ] {
        let out = load(&rules)?;
        assert!(passed(&out), "{rules}: {out:?}");
    }
    Ok(())
}

#[test]
fn indistinct_two_ty_rules_without_a_name_or_with_one_name_is_a_load_fault() -> Outcome {
    for (a, b, key) in [
        ("{engine: ty}", "{engine: ty}", "ty"),
        ("{engine: ty, rule: x}", "{engine: ty, rule: x}", "ty `x`"),
    ] {
        let out = load(&format!("{}{}", rule("t.one", a), rule("t.two", b)))?;
        assert_eq!(out.code, 2, "{key}: {out:?}");
        assert!(
            out.stderr
                .contains(&format!("rules `t.one` and `t.two` both take every {key} diagnostic")),
            "{key}: {}",
            out.stderr
        );
    }
    Ok(())
}

#[test]
fn indistinct_ty_named_rules_beside_an_unnamed_one_load() -> Outcome {
    for rules in [
        format!(
            "{}{}",
            rule("t.one", "{engine: ty, rule: x}"),
            rule("t.two", "{engine: ty}")
        ),
        format!(
            "{}{}",
            rule("t.one", "{engine: ty, rule: x}"),
            rule("t.two", "{engine: ty, rule: other}")
        ),
    ] {
        let out = load(&rules)?;
        assert!(passed(&out), "{rules}: {out:?}");
    }
    Ok(())
}

#[test]
fn indistinct_holds_across_catalogues_and_is_found_before_any_tool_runs() -> Outcome {
    let project = Project::with_rules("[\"pkg\"]", &rule("t.one", "{engine: ruff, code: T201}"))?;
    project.config("roots = [\"pkg\"]\ncatalogues = [\"cat/\", \"more/\"]\n")?;
    project.file(
        "more/catalogue.toml",
        common::MANIFEST.replace("\"t\"", "\"more\"").as_str(),
    )?;
    project.file("more/rules/r.yml", &rule("u.two", "{engine: ruff, code: T201}"))?;
    project.file("pkg/a.py", "x = 1\n")?;
    let empty = project.path().join("nothing");
    fs::create_dir_all(&empty)?;
    let out = project.check_on(&only(&[&empty])?, &[])?;
    assert_eq!(out.code, 2, "{out:?}");
    assert!(
        out.stderr.contains("both take every ruff `T201` diagnostic"),
        "{}",
        out.stderr
    );
    assert!(!out.stderr.contains("not found"), "{}", out.stderr);
    Ok(())
}

#[test]
fn indistinct_a_code_that_is_not_a_code_is_refused_at_load() -> Outcome {
    let out = load(&rule("t.one", "{engine: ruff, code: t201}"))?;
    assert_eq!(out.code, 2, "{out:?}");
    assert!(out.stderr.contains("not a code"), "{}", out.stderr);
    Ok(())
}
