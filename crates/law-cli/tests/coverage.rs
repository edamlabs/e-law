mod common;

use std::collections::BTreeMap;
use std::fs;

use common::{Out, Outcome, Project};
use serde_json::Value;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/coverage");
const EAGER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../catalogues/eager/catalogue.toml");
const UNTAGGED: &str = "(untagged)";
const LAWS: usize = 26;

type Fault = Box<dyn std::error::Error>;

#[derive(Default)]
struct Counted {
    rules: u64,
    engines: BTreeMap<String, u64>,
    fixes: [u64; 3],
    untestable: u64,
}

fn catalogue() -> String {
    format!("{FIXTURES}/cat")
}

fn coverage(project: &Project, catalogue: &str, by: &str, format: &str) -> Result<Out, Fault> {
    project.run(&[
        "coverage",
        "--catalogue",
        catalogue,
        "--by",
        by,
        "--output-format",
        format,
    ])
}

fn golden(by: &str, format: &str) -> Result<String, Fault> {
    Ok(fs::read_to_string(format!("{FIXTURES}/{by}.{format}"))?)
}

fn rows(json: &Value) -> Result<&Vec<Value>, Fault> {
    Ok(json["rows"].as_array().ok_or("no rows")?)
}

fn values(json: &Value) -> Result<Vec<&str>, Fault> {
    rows(json)?
        .iter()
        .map(|row| row["value"].as_str().ok_or_else(|| "no value".into()))
        .collect()
}

fn tagged(count: usize) -> String {
    (0..count)
        .map(|index| {
            format!(
                "- id: t.r{index}\n  message: m\n  tags: {{law: \"{}\"}}\n  check: {{engine: pattern, rule: {{kind: lambda}}}}\n  fix: {{safety: none, reason: x}}\n",
                [100, 9, 11, 10, 2, 1000][index % 6] + index / 6 * 7
            )
        })
        .collect::<Vec<_>>()
        .concat()
}

#[test]
fn coverage_each_format_of_a_fixture_catalogue_is_its_golden_file() -> Outcome {
    let project = Project::new()?;
    for (by, format) in [
        ("law", "text"),
        ("law", "md"),
        ("law", "json"),
        ("set", "text"),
        ("tier", "text"),
        ("kit", "text"),
    ] {
        let out = coverage(&project, &catalogue(), by, format)?;
        assert_eq!((out.code, out.stderr.as_str()), (0, ""), "{by} {format}");
        assert_eq!(out.stdout, golden(by, format)?, "{by} {format}");
    }
    Ok(())
}

#[test]
fn coverage_default_key_is_law_and_default_format_is_text() -> Outcome {
    let project = Project::new()?;
    let out = project.run(&["coverage", "--catalogue", &catalogue()])?;
    assert_eq!(out.stdout, golden("law", "text")?);
    Ok(())
}

#[test]
fn coverage_values_sort_numerically_when_every_one_is_a_number() -> Outcome {
    let project = Project::new()?;
    let json: Value = serde_json::from_str(&coverage(&project, &catalogue(), "law", "json")?.stdout)?;
    assert_eq!(values(&json)?, ["2", "3", "10", UNTAGGED]);
    let many = Project::with_rules("[\"src\"]", &tagged(12))?;
    let json: Value = serde_json::from_str(&coverage(&many, "cat/", "law", "json")?.stdout)?;
    let numbers: Vec<u64> = values(&json)?
        .iter()
        .map(|value| value.parse())
        .collect::<Result<_, _>>()?;
    assert!(numbers.len() > 6 && numbers.is_sorted(), "{numbers:?}");
    assert!(numbers.contains(&9) && numbers.contains(&1000), "{numbers:?}");
    Ok(())
}

#[test]
fn coverage_values_sort_lexically_when_one_is_not_a_number() -> Outcome {
    let project = Project::new()?;
    let json: Value = serde_json::from_str(&coverage(&project, &catalogue(), "tier", "json")?.stdout)?;
    assert_eq!(values(&json)?, ["10", "2", "x", UNTAGGED]);
    let json: Value = serde_json::from_str(&coverage(&project, &catalogue(), "set", "json")?.stdout)?;
    assert_eq!(values(&json)?, ["A", "B", UNTAGGED]);
    Ok(())
}

#[test]
fn coverage_rules_lacking_the_key_are_a_last_untagged_row_and_only_then() -> Outcome {
    let project = Project::new()?;
    let json: Value = serde_json::from_str(&coverage(&project, &catalogue(), "law", "json")?.stdout)?;
    let last = rows(&json)?.last().ok_or("no row")?;
    assert_eq!(
        (last["value"].as_str(), last["rules"].as_u64()),
        (Some(UNTAGGED), Some(2))
    );
    let all = Project::with_rules("[\"src\"]", &tagged(3))?;
    let json: Value = serde_json::from_str(&coverage(&all, "cat/", "law", "json")?.stdout)?;
    assert!(!values(&json)?.contains(&UNTAGGED));
    let none: Value = serde_json::from_str(&coverage(&project, &catalogue(), "nokey", "json")?.stdout)?;
    assert_eq!(values(&none)?, [UNTAGGED]);
    assert_eq!(rows(&none)?[0]["rules"], 7);
    Ok(())
}

#[test]
fn coverage_a_rule_with_two_values_is_counted_under_each() -> Outcome {
    let project = Project::new()?;
    let json: Value = serde_json::from_str(&coverage(&project, &catalogue(), "set", "json")?.stdout)?;
    let by: BTreeMap<&str, u64> = rows(&json)?
        .iter()
        .filter_map(|row| Some((row["value"].as_str()?, row["rules"].as_u64()?)))
        .collect();
    assert_eq!(by, BTreeMap::from([("A", 2), ("B", 4), (UNTAGGED, 2)]));
    Ok(())
}

#[test]
fn coverage_counts_engines_fixes_and_the_rules_law_test_cannot_prove() -> Outcome {
    let project = Project::new()?;
    let json: Value = serde_json::from_str(&coverage(&project, &catalogue(), "law", "json")?.stdout)?;
    let ten = &rows(&json)?[2];
    assert_eq!(ten["value"], "10");
    assert_eq!(ten["engines"], serde_json::json!({"ruff": 1, "tree": 1}));
    assert_eq!(ten["fixable"], serde_json::json!({"safe": 0, "unsafe": 1, "none": 1}));
    assert_eq!(ten["untestable"], 1);
    let two = &rows(&json)?[0];
    assert_eq!(two["fixable"], serde_json::json!({"safe": 1, "unsafe": 0, "none": 1}));
    assert_eq!(two["untestable"], 0);
    Ok(())
}

#[test]
fn coverage_prose_is_listed_under_the_table_in_the_order_of_its_values() -> Outcome {
    let project = Project::new()?;
    let text = coverage(&project, &catalogue(), "law", "text")?.stdout;
    let prose: Vec<&str> = text
        .lines()
        .skip_while(|line| *line != "Stays prose:")
        .skip(1)
        .collect();
    assert_eq!(
        prose,
        [
            "  law 2: no rule yet: pattern (membership in a list)",
            "  law 6: surveying happens before the code exists",
            "  law 10: no rule yet: graph (dead privates)",
            "  (untagged): carries no law at all"
        ]
    );
    let md = coverage(&project, &catalogue(), "law", "md")?.stdout;
    assert!(md.contains("\nStays prose:\n- law 2: no rule yet: pattern (membership in a list)\n- law 6: surveying happens before the code exists\n- law 10: no rule yet: graph (dead privates)\n- (untagged): carries no law at all\n"), "{md}");
    Ok(())
}

#[test]
fn coverage_a_value_with_a_prose_entry_and_no_rule_is_not_a_row() -> Outcome {
    let project = Project::new()?;
    let json: Value = serde_json::from_str(&coverage(&project, &catalogue(), "law", "json")?.stdout)?;
    assert!(!values(&json)?.contains(&"6"));
    assert!(
        json["prose"]
            .as_array()
            .ok_or("prose")?
            .iter()
            .any(|entry| entry["values"] == serde_json::json!(["6"]))
    );
    Ok(())
}

#[test]
fn coverage_without_prose_has_no_stays_prose_section() -> Outcome {
    let project = Project::with_rules("[\"src\"]", &tagged(2))?;
    let out = coverage(&project, "cat/", "law", "text")?;
    assert!(!out.stdout.contains("Stays prose"), "{}", out.stdout);
    let json: Value = serde_json::from_str(&coverage(&project, "cat/", "law", "json")?.stdout)?;
    assert_eq!(json["prose"], serde_json::json!([]));
    Ok(())
}

#[test]
fn coverage_the_selection_narrows_the_rules_counted() -> Outcome {
    let project = Project::new()?;
    let out = project.run(&[
        "coverage",
        "--catalogue",
        &catalogue(),
        "--select",
        "fx.alpha",
        "--output-format",
        "json",
    ])?;
    let json: Value = serde_json::from_str(&out.stdout)?;
    assert_eq!(values(&json)?, ["2"]);
    assert_eq!(rows(&json)?[0]["rules"], 1);
    Ok(())
}

#[test]
fn coverage_the_configured_catalogues_are_counted_without_the_flag() -> Outcome {
    let project = Project::with_rules("[\"src\"]", &tagged(3))?;
    let out = project.run(&["coverage", "--output-format", "json"])?;
    assert_eq!(out.code, 0, "{}", out.stderr);
    let json: Value = serde_json::from_str(&out.stdout)?;
    assert_eq!(
        rows(&json)?
            .iter()
            .map(|row| row["rules"].as_u64().unwrap_or_default())
            .sum::<u64>(),
        3
    );
    Ok(())
}

#[test]
fn coverage_is_exit_2_for_a_missing_catalogue_and_an_unknown_format() -> Outcome {
    let project = Project::new()?;
    let absent = coverage(&project, "absent/", "law", "text")?;
    assert_eq!((absent.code, absent.stdout.as_str()), (2, ""));
    assert!(!absent.stderr.is_empty());
    let format = coverage(&project, &catalogue(), "law", "sarif")?;
    assert_eq!((format.code, format.stdout.as_str()), (2, ""));
    assert!(format.stderr.contains("sarif"), "{}", format.stderr);
    Ok(())
}

#[test]
fn coverage_md_and_text_carry_the_rows_of_the_json() -> Outcome {
    let project = Project::new()?;
    for by in ["law", "set", "tier"] {
        let json: Value = serde_json::from_str(&coverage(&project, &catalogue(), by, "json")?.stdout)?;
        let md = coverage(&project, &catalogue(), by, "md")?.stdout;
        let text = coverage(&project, &catalogue(), by, "text")?.stdout;
        let cells = |output: &str, split: fn(&str) -> Option<String>| -> Vec<String> {
            output.lines().filter_map(split).collect()
        };
        let in_md = cells(&md, |line| {
            line.strip_prefix("| ")
                .and_then(|rest| rest.split(" | ").next())
                .map(str::to_owned)
        });
        let in_text = cells(&text, |line| {
            (!line.starts_with(' '))
                .then(|| line.split_whitespace().next().map(str::to_owned))
                .flatten()
        });
        let expected: Vec<String> = std::iter::once(by.to_owned())
            .chain(values(&json)?.into_iter().map(str::to_owned))
            .collect();
        assert_eq!(
            in_md.iter().filter(|cell| *cell != "---").cloned().collect::<Vec<_>>(),
            expected,
            "{by} md"
        );
        assert_eq!(
            in_text
                .iter()
                .filter(|cell| !cell.is_empty() && *cell != "Stays")
                .cloned()
                .collect::<Vec<_>>(),
            expected
                .iter()
                .map(|value| value.split(' ').next().unwrap_or_default().to_owned())
                .collect::<Vec<_>>(),
            "{by} text"
        );
    }
    Ok(())
}

#[test]
fn coverage_eager_counts_equal_the_rules_grouped_by_hand() -> Outcome {
    let project = Project::new()?;
    let listed: Value = serde_json::from_str(
        &project
            .run(&["rule", "--catalogue", "eager", "--output-format", "json"])?
            .stdout,
    )?;
    let mut by: BTreeMap<String, Counted> = BTreeMap::new();
    for rule in listed.as_array().ok_or("rules")? {
        let law = rule["tags"]["law"]
            .as_str()
            .ok_or("a rule of the eager catalogue has no law")?;
        let engine = rule["check"]["engine"].as_str().ok_or("engine")?;
        let entry = by.entry(law.to_owned()).or_default();
        entry.rules += 1;
        *entry.engines.entry(engine.to_owned()).or_default() += 1;
        match rule["fix"]["safety"].as_str().ok_or("safety")? {
            "safe" => entry.fixes[0] += 1,
            "unsafe" => entry.fixes[1] += 1,
            _ => entry.fixes[2] += 1,
        }
        entry.untestable += u64::from(matches!(engine, "tree" | "ty" | "tach"));
    }
    let json: Value = serde_json::from_str(&coverage(&project, "eager", "law", "json")?.stdout)?;
    let mut order: Vec<&String> = by.keys().collect();
    order.sort_by_key(|law| law.parse::<u64>().unwrap_or_default());
    assert_eq!(values(&json)?, order.iter().map(|law| law.as_str()).collect::<Vec<_>>());
    assert_eq!(
        listed.as_array().ok_or("rules")?.len() as u64,
        by.values().map(|entry| entry.rules).sum::<u64>()
    );
    for row in rows(&json)? {
        let law = row["value"].as_str().ok_or("value")?;
        let Counted {
            rules,
            engines,
            fixes,
            untestable,
        } = &by[law];
        assert_eq!(row["rules"], *rules, "law {law}");
        let counted: BTreeMap<String, u64> = row["engines"]
            .as_object()
            .ok_or("engines")?
            .iter()
            .filter_map(|(engine, count)| Some((engine.clone(), count.as_u64()?)))
            .collect();
        assert_eq!(&counted, engines, "law {law}");
        assert_eq!(
            [
                row["fixable"]["safe"].as_u64(),
                row["fixable"]["unsafe"].as_u64(),
                row["fixable"]["none"].as_u64()
            ],
            fixes.map(Some),
            "law {law}"
        );
        assert_eq!(row["untestable"], *untestable, "law {law}");
    }
    Ok(())
}

#[test]
fn coverage_eager_every_law_is_a_row_or_prose_and_the_prose_is_the_catalogues() -> Outcome {
    let project = Project::new()?;
    let json: Value = serde_json::from_str(&coverage(&project, "eager", "law", "json")?.stdout)?;
    let declared = fs::read_to_string(EAGER)?.matches("[[prose]]").count();
    let prose = json["prose"].as_array().ok_or("prose")?;
    assert_eq!(prose.len(), declared);
    assert_eq!(declared, LAWS);
    let mut known: Vec<u64> = values(&json)?.iter().filter_map(|value| value.parse().ok()).collect();
    known.extend(
        prose
            .iter()
            .filter_map(|entry| entry["values"][0].as_str()?.parse::<u64>().ok()),
    );
    known.sort_unstable();
    known.dedup();
    assert_eq!(known, (1..=LAWS as u64).collect::<Vec<_>>());
    assert!(prose.iter().all(|entry| {
        entry["text"]
            .as_str()
            .is_some_and(|text| !text.is_empty() && !text.contains('\n'))
    }));
    let text = coverage(&project, "eager", "law", "text")?.stdout;
    assert_eq!(text.lines().filter(|line| line.starts_with("  law ")).count(), LAWS);
    Ok(())
}

#[test]
fn coverage_eager_rows_are_listed_numerically_in_the_text_table() -> Outcome {
    let project = Project::new()?;
    let text = coverage(&project, "eager", "law", "text")?.stdout;
    let laws: Vec<u64> = text
        .lines()
        .skip(1)
        .take_while(|line| !line.is_empty())
        .filter_map(|line| line.split_whitespace().next()?.parse().ok())
        .collect();
    assert!(laws.len() > 10 && laws.is_sorted(), "{laws:?}");
    assert!(laws.windows(2).any(|pair| pair[0] < 10 && pair[1] >= 10), "{laws:?}");
    Ok(())
}
