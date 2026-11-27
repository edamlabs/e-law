mod common;

use std::path::{Path, PathBuf};

use common::{Outcome, write};
use law_config::{Config, Fault, Project, Source, discover};
use law_model::Selector;

const LAW: &str = include_str!("fixtures/law.toml");
const PYPROJECT: &str = include_str!("fixtures/pyproject.toml");

fn selectors(texts: &[&str]) -> Result<Vec<Selector>, Box<dyn std::error::Error>> {
    Ok(texts.iter().map(|text| text.parse()).collect::<Result<_, _>>()?)
}

fn parse(text: &str) -> Result<Config, Box<dyn std::error::Error>> {
    Config::parse(text, Path::new("law.toml"))?.ok_or_else(|| "no config".into())
}

fn found(start: &Path) -> Result<Option<Project>, Box<dyn std::error::Error>> {
    discover(start).map_err(|faults| format!("{faults:?}").into())
}

fn refused(text: &str) -> bool {
    Config::parse(text, Path::new("law.toml")).is_err()
}

fn check_sample(config: &Config) -> Outcome {
    assert_eq!(config.roots, [PathBuf::from("src"), PathBuf::from("tests")]);
    assert_eq!(
        config.catalogues,
        [Source::Builtin("eager".into()), Source::Dir(PathBuf::from("law/"))]
    );
    assert_eq!(config.select, selectors(&["ALL"])?);
    assert_eq!(config.ignore, selectors(&["law=prompts", "lexis.loop"])?);
    assert_eq!(config.exceptions.as_deref(), Some(Path::new("law.exceptions.toml")));
    let engine = config.profiles.get(&"engine".parse()?).ok_or("no engine profile")?;
    assert_eq!(engine.thresholds.get("lines"), Some(&400));
    Ok(())
}

#[test]
fn config_sample_parses_as_law_toml() -> Outcome {
    check_sample(&parse(LAW)?)
}

#[test]
fn config_sample_parses_under_tool_law_in_pyproject() -> Outcome {
    let config = Config::parse(PYPROJECT, Path::new("pyproject.toml"))?.ok_or("no [tool.law]")?;
    check_sample(&config)
}

#[test]
fn config_pyproject_and_law_toml_samples_are_equal() -> Outcome {
    let law = parse(LAW)?;
    let pyproject = Config::parse(PYPROJECT, Path::new("pyproject.toml"))?.ok_or("none")?;
    assert_eq!(law, pyproject);
    Ok(())
}

#[test]
fn config_pyproject_without_tool_law_is_none() -> Outcome {
    assert_eq!(
        Config::parse(
            "[project]\nname = \"x\"\n[tool.ruff]\nline-length = 100\n",
            Path::new("pyproject.toml")
        )?,
        None
    );
    Ok(())
}

#[test]
fn config_pyproject_other_tools_do_not_matter() -> Outcome {
    let text = format!("[tool.ruff]\nline-length = 100\n{PYPROJECT}");
    assert!(Config::parse(&text, Path::new("pyproject.toml"))?.is_some());
    Ok(())
}

#[test]
fn config_empty_takes_defaults() -> Outcome {
    let config = parse("")?;
    assert_eq!(config, Config::default());
    assert_eq!(config.roots, [PathBuf::from(".")]);
    assert_eq!(config.select, [Selector::All]);
    assert!(config.ignore.is_empty() && config.catalogues.is_empty() && config.exceptions.is_none());
    Ok(())
}

#[test]
fn config_sample_round_trips_through_toml() -> Outcome {
    let config = parse(LAW)?;
    let back = parse(&toml::to_string(&config)?)?;
    assert_eq!(back, config);
    Ok(())
}

#[test]
fn config_default_round_trips_through_toml() -> Outcome {
    let config = Config::default();
    assert_eq!(parse(&toml::to_string(&config)?)?, config);
    Ok(())
}

#[test]
fn config_source_directory_without_slash_is_a_builtin_name() -> Outcome {
    let config = parse("catalogues = [\"eager\", \"./law\", \"law/\", \"vendor/rules\"]\n")?;
    assert_eq!(
        config.catalogues,
        [
            Source::Builtin("eager".into()),
            Source::Dir(PathBuf::from("./law")),
            Source::Dir(PathBuf::from("law/")),
            Source::Dir(PathBuf::from("vendor/rules")),
        ]
    );
    Ok(())
}

#[test]
fn config_refuses_unknown_top_level_key() {
    assert!(refused("rots = [\"src\"]\n"));
}

#[test]
fn config_refuses_unknown_key_in_profile_overrides() {
    assert!(refused("[profiles.engine]\nlines = 3\n"));
}

#[test]
fn config_refuses_bad_selectors() {
    for bad in ["Lexis", "=5", "law=", "lexis"] {
        assert!(refused(&format!("select = [\"{bad}\"]\n")), "{bad}");
        assert!(refused(&format!("ignore = [\"{bad}\"]\n")), "{bad}");
    }
}

#[test]
fn config_refuses_bad_profile_kind_and_threshold_type() {
    assert!(refused("[profiles.Engine.thresholds]\nlines = 1\n"));
    assert!(refused("[profiles.engine.thresholds]\nlines = \"many\"\n"));
    assert!(refused("[profiles.engine.thresholds]\nlines = -1\n"));
}

#[test]
fn config_parse_fault_names_path_and_line() -> Outcome {
    let fault = Config::parse("roots = [\"src\"]\nselect = [\"=5\"]\n", Path::new("law.toml"))
        .err()
        .ok_or("parsed")?;
    assert!(
        matches!(&fault, Fault::Invalid { path, line: Some(2), .. } if path == Path::new("law.toml")),
        "{fault:?}"
    );
    assert!(fault.to_string().starts_with("law.toml:2: "), "{fault}");
    let fault = Config::parse("roots = 1\n", Path::new("law.toml"))
        .err()
        .ok_or("parsed")?;
    assert!(matches!(&fault, Fault::Parse { line: Some(1), .. }), "{fault:?}");
    Ok(())
}

#[test]
fn project_read_roots_at_the_files_directory() -> Outcome {
    let tmp = tempfile::tempdir()?;
    let path = write(tmp.path(), "law.toml", LAW)?;
    let project = Project::read(&path)?.ok_or("none")?;
    assert_eq!(project.root, tmp.path());
    assert_eq!(project.path, path);
    check_sample(&project.config)
}

#[test]
fn project_read_of_a_missing_file_is_an_io_fault() {
    let fault = Project::read(Path::new("/nonexistent/law.toml")).err();
    assert!(matches!(fault, Some(Fault::Io { .. })), "{fault:?}");
}

#[test]
fn discover_walks_up_to_the_first_law_toml() -> Outcome {
    let tmp = tempfile::tempdir()?;
    write(tmp.path(), "law.toml", "")?;
    write(tmp.path(), "a/b/c/keep", "")?;
    let project = found(&tmp.path().join("a/b/c"))?.ok_or("none")?;
    assert_eq!(project.root, tmp.path());
    Ok(())
}

#[test]
fn discover_takes_the_nearest_directory() -> Outcome {
    let tmp = tempfile::tempdir()?;
    write(tmp.path(), "law.toml", "roots = [\"outer\"]\n")?;
    write(tmp.path(), "a/law.toml", "roots = [\"inner\"]\n")?;
    let project = found(&tmp.path().join("a/b"))?.ok_or("none")?;
    assert_eq!(project.root, tmp.path().join("a"));
    assert_eq!(project.config.roots, [PathBuf::from("inner")]);
    Ok(())
}

#[test]
fn discover_finds_pyproject_with_tool_law() -> Outcome {
    let tmp = tempfile::tempdir()?;
    write(tmp.path(), "pyproject.toml", PYPROJECT)?;
    let project = found(tmp.path())?.ok_or("none")?;
    assert_eq!(project.path, tmp.path().join("pyproject.toml"));
    Ok(())
}

#[test]
fn discover_skips_pyproject_without_tool_law() -> Outcome {
    let tmp = tempfile::tempdir()?;
    write(tmp.path(), "law.toml", "")?;
    write(tmp.path(), "pkg/pyproject.toml", "[project]\nname = \"x\"\n")?;
    let project = found(&tmp.path().join("pkg"))?.ok_or("none")?;
    assert_eq!(project.root, tmp.path());
    Ok(())
}

#[test]
fn discover_with_both_configs_is_a_two_configs_fault() -> Outcome {
    let tmp = tempfile::tempdir()?;
    write(tmp.path(), "law.toml", LAW)?;
    write(tmp.path(), "pyproject.toml", PYPROJECT)?;
    let faults = discover(tmp.path()).err().ok_or("discovered")?;
    assert!(
        matches!(faults.as_slice(), [Fault::TwoConfigs { law, pyproject }]
            if law.ends_with("law.toml") && pyproject.ends_with("pyproject.toml")),
        "{faults:?}"
    );
    Ok(())
}

#[test]
fn discover_two_configs_fault_displays_as_path_what() -> Outcome {
    let tmp = tempfile::tempdir()?;
    write(tmp.path(), "law.toml", "")?;
    write(tmp.path(), "pyproject.toml", PYPROJECT)?;
    let faults = discover(tmp.path()).err().ok_or("discovered")?;
    let text = faults.first().ok_or("none")?.to_string();
    assert!(
        text.starts_with(&tmp.path().join("law.toml").display().to_string()),
        "{text}"
    );
    assert!(text.contains("two configurations"), "{text}");
    Ok(())
}

#[test]
fn discover_law_toml_beside_a_pyproject_without_tool_law_is_not_two_configs() -> Outcome {
    let tmp = tempfile::tempdir()?;
    write(tmp.path(), "law.toml", "")?;
    write(tmp.path(), "pyproject.toml", "[project]\nname = \"x\"\n")?;
    assert!(found(tmp.path())?.is_some());
    Ok(())
}

#[test]
fn discover_reports_a_bad_config_instead_of_skipping_it() -> Outcome {
    let tmp = tempfile::tempdir()?;
    write(tmp.path(), "law.toml", "roots = 1\n")?;
    let faults = discover(tmp.path()).err().ok_or("discovered")?;
    assert!(matches!(faults.as_slice(), [Fault::Parse { .. }]), "{faults:?}");
    Ok(())
}

#[test]
fn discover_collects_a_bad_law_toml_and_a_bad_pyproject_together() -> Outcome {
    let tmp = tempfile::tempdir()?;
    write(tmp.path(), "law.toml", "roots = 1\n")?;
    write(tmp.path(), "pyproject.toml", "[tool.law]\nroots = 2\n")?;
    let faults = discover(tmp.path()).err().ok_or("discovered")?;
    assert_eq!(faults.len(), 2, "{faults:?}");
    Ok(())
}
