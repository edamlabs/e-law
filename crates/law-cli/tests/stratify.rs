mod common;

use common::{Outcome, Project};
use serde_json::Value;

type Fault = Box<dyn std::error::Error>;

fn lines(count: usize) -> String {
    "x = 1\n".repeat(count)
}

fn classes(count: usize, suffix: &str) -> String {
    (0..count).fold(String::new(), |all, at| {
        all + &format!("class Row{at}{suffix}: ...\n\n")
    })
}

fn world(files: &[(&str, String)]) -> Result<Project, Fault> {
    let project = Project::new()?;
    project.config("roots = [\"src\"]\ncatalogues = [\"eager\"]\n")?;
    for (path, text) in files {
        project.file(path, text)?;
    }
    Ok(project)
}

fn flagged(project: &Project, id: &str) -> Result<Vec<(String, String)>, Fault> {
    let report = project.json(&["--select", id])?;
    Ok(report["findings"]
        .as_array()
        .ok_or("no findings")?
        .iter()
        .filter_map(|finding| {
            Some((
                finding["path"].as_str()?.to_owned(),
                finding["message"].as_str()?.rsplit(": ").next()?.to_owned(),
            ))
        })
        .collect())
}

fn at(path: &str, detail: &str) -> Vec<(String, String)> {
    vec![(path.to_owned(), detail.to_owned())]
}

#[test]
fn stratify_entity_lines_bite_past_500_and_not_at_500() -> Outcome {
    let at_limit = world(&[("src/pkg/engines/cache.py", lines(500))])?;
    assert_eq!(flagged(&at_limit, "stratify.entity-lines")?, Vec::new());
    let over = world(&[("src/pkg/engines/cache.py", lines(501))])?;
    assert_eq!(
        flagged(&over, "stratify.entity-lines")?,
        at("src/pkg/engines/cache.py", "501 lines, over 500")
    );
    Ok(())
}

#[test]
fn stratify_entity_companions_bite_past_3_and_not_at_3() -> Outcome {
    let at_limit = world(&[("src/pkg/engines/cache.py", classes(3, ""))])?;
    assert_eq!(flagged(&at_limit, "stratify.entity-companions")?, Vec::new());
    let over = world(&[("src/pkg/engines/cache.py", classes(4, ""))])?;
    assert_eq!(
        flagged(&over, "stratify.entity-companions")?,
        at("src/pkg/engines/cache.py", "4 companions, over 3")
    );
    Ok(())
}

#[test]
fn stratify_parts_bite_past_16_classes_and_past_500_lines() -> Outcome {
    let at_limit = world(&[("src/pkg/engines/cache/models.py", classes(16, ""))])?;
    assert_eq!(flagged(&at_limit, "stratify.part-units")?, Vec::new());
    let over = world(&[("src/pkg/engines/cache/models.py", classes(17, ""))])?;
    assert_eq!(
        flagged(&over, "stratify.part-units")?,
        at("src/pkg/engines/cache/models.py", "17 classes, over 16")
    );
    let long = world(&[("src/pkg/processors/scan/nodes.py", lines(501))])?;
    assert_eq!(
        flagged(&long, "stratify.part-lines")?,
        at("src/pkg/processors/scan/nodes.py", "501 lines, over 500")
    );
    Ok(())
}

#[test]
fn stratify_a_split_entity_is_still_judged_by_its_profile_in_the_package_form() -> Outcome {
    let project = world(&[
        ("src/pkg/providers/drivers/db/pg/driver.py", lines(501)),
        ("src/pkg/providers/adapters/vec/lance/adapter.py", lines(501)),
        ("src/pkg/providers/drivers/db/pg/models.py", lines(501)),
        ("src/pkg/providers/drivers/db/other.py", lines(501)),
    ])?;
    let found = flagged(&project, "stratify.entity-lines")?;
    assert_eq!(
        found,
        [
            ("src/pkg/providers/adapters/vec/lance/adapter.py", "501 lines, over 500"),
            ("src/pkg/providers/drivers/db/other.py", "501 lines, over 500"),
            ("src/pkg/providers/drivers/db/pg/driver.py", "501 lines, over 500"),
        ]
        .map(|(path, detail)| (path.to_owned(), detail.to_owned()))
    );
    Ok(())
}

#[test]
fn stratify_a_file_of_a_models_or_nodes_folder_holds_one_unit() -> Outcome {
    let project = world(&[
        ("src/pkg/engines/cache/models/row.py", classes(2, "")),
        ("src/pkg/engines/cache/models/kind.py", classes(1, "")),
        ("src/pkg/processors/scan/nodes/read.py", classes(3, "Node")),
        ("src/pkg/engines/cache/models.py", classes(2, "")),
    ])?;
    assert_eq!(
        flagged(&project, "stratify.one-unit")?,
        [
            ("src/pkg/engines/cache/models/row.py", "2 classes, over 1"),
            ("src/pkg/processors/scan/nodes/read.py", "3 classes, over 1"),
        ]
        .map(|(path, detail)| (path.to_owned(), detail.to_owned()))
    );
    Ok(())
}

#[test]
fn stratify_a_project_tunes_the_defaults_in_its_config() -> Outcome {
    let project = world(&[("src/pkg/engines/cache.py", lines(120))])?;
    project.config("roots = [\"src\"]\ncatalogues = [\"eager\"]\n\n[profiles.engine.thresholds]\nlines = 100\n")?;
    assert_eq!(
        flagged(&project, "stratify.entity-lines")?,
        at("src/pkg/engines/cache.py", "120 lines, over 100")
    );
    Ok(())
}

#[test]
fn stratify_only_the_entity_rules_carry_a_fix_and_it_is_an_unsafe_move() -> Outcome {
    let project = Project::new()?;
    let out = project.run(&["rule", "--output-format", "json", "--catalogue", "eager"])?;
    let all: Vec<Value> = serde_json::from_str(&out.stdout)?;
    let fixes: Vec<(String, String, bool)> = all
        .iter()
        .filter(|rule| rule["id"].as_str().is_some_and(|id| id.starts_with("stratify.")))
        .filter_map(|rule| {
            Some((
                rule["id"].as_str()?.to_owned(),
                rule["fix"]["safety"].as_str()?.to_owned(),
                rule["check"]["relocate"] == "package",
            ))
        })
        .collect();
    assert_eq!(
        fixes,
        [
            ("stratify.entity-lines", "unsafe", true),
            ("stratify.entity-companions", "unsafe", true),
            ("stratify.part-units", "none", false),
            ("stratify.part-lines", "none", false),
            ("stratify.one-unit", "none", false),
        ]
        .map(|(id, safety, moves)| (id.to_owned(), safety.to_owned(), moves))
    );
    Ok(())
}

#[test]
fn stratify_the_fix_moves_the_entity_and_its_importers_and_the_next_check_offers_none() -> Outcome {
    let project = world(&[
        ("src/pkg/engines/cache.py", lines(501)),
        ("src/pkg/app.py", "from pkg.engines.cache import x0\n".to_owned()),
    ])?;
    let id = "stratify.entity-lines";
    let kept = project.check(&["--select", id, "--fix"])?;
    assert_eq!(kept.code, 0, "{kept:?}");
    assert!(project.path().join("src/pkg/engines/cache.py").exists());
    let done = project.check(&["--select", id, "--fix", "--unsafe-fixes"])?;
    assert_eq!(done.code, 0, "{done:?}");
    assert!(!project.path().join("src/pkg/engines/cache.py").exists());
    assert_eq!(
        std::fs::read_to_string(project.path().join("src/pkg/app.py"))?,
        "from pkg.engines.cache.engine import x0\n"
    );
    let again = project.json(&["--select", id])?;
    let findings = again["findings"].as_array().ok_or("no findings")?;
    assert_eq!(findings.len(), 1, "{again}");
    assert_eq!(findings[0]["path"], "src/pkg/engines/cache/engine.py");
    assert!(findings[0]["fix"].is_null());
    Ok(())
}
