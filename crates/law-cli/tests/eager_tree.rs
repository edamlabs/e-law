mod common;

use common::{Outcome, Project, passed};

fn project(files: &[&str]) -> Result<Project, Box<dyn std::error::Error>> {
    let project = Project::new()?;
    project.config("catalogues = [\"eager\"]\n")?;
    for file in files {
        project.file(file, "")?;
    }
    Ok(project)
}

fn found(files: &[&str], id: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let out = project(files)?.check(&["--select", id])?;
    Ok(out
        .stdout
        .lines()
        .filter(|line| line.contains(&format!(": {id} ")))
        .map(|line| line.rsplit(": ").next().unwrap_or_default().to_owned())
        .collect())
}

#[test]
fn eager_single_word_module_flags_an_underscore_in_a_module_or_a_package() -> Outcome {
    let id = "ost.single-word-module";
    let bad = found(
        &[
            "src/e_pkg/engines/doc_processor.py",
            "src/e_pkg/my_engines/cache.py",
            "src/e_pkg/engines/cache.py",
        ],
        id,
    )?;
    assert_eq!(bad.len(), 2, "{bad:?}");
    assert!(bad.iter().any(|line| line.starts_with("doc_processor.py ")), "{bad:?}");
    assert!(bad.iter().any(|line| line.starts_with("my_engines/ ")), "{bad:?}");
    Ok(())
}

#[test]
fn eager_single_word_module_spares_the_package_name_dunders_and_test_files() -> Outcome {
    let bad = found(
        &[
            "src/e_pkg/__main__.py",
            "src/e_pkg/engines/cache.py",
            "src/e_pkg/engines/py.typed",
            "tests/unit/e_pkg/engines/test_cache_ttl.py",
            "tests/unit/e_pkg/engines/conftest.py",
            "tests/unit/e_pkg/test___main__.py",
        ],
        "ost.single-word-module",
    )?;
    assert!(bad.is_empty(), "{bad:?}");
    Ok(())
}

#[test]
fn eager_single_word_module_reaches_every_test_tier_and_spares_the_assets() -> Outcome {
    let id = "ost.single-word-module";
    let bad = found(
        &[
            "tests/unit/e_pkg/engines/doc_processor.py",
            "tests/integration/e_pkg/my_engines/cache.py",
            "tests/evaluation/e_pkg/doc_processor.py",
            "tests/acceptance/e_pkg/doc_processor.py",
            "tests/benchmark/e_pkg/doc_processor.py",
            "tests/benchmark/e_pkg/test_cache_ttl.py",
            "tests/resources/sample_doc.py",
            "tests/sandbox/sample_compose.py",
        ],
        id,
    )?;
    let mut names: Vec<&str> = bad.iter().filter_map(|line| line.split(' ').next()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "doc_processor.py",
            "doc_processor.py",
            "doc_processor.py",
            "doc_processor.py",
            "my_engines/"
        ],
        "{bad:?}"
    );
    Ok(())
}

#[test]
fn eager_single_word_module_is_a_python_file_or_a_folder_never_another_file() -> Outcome {
    let bad = found(
        &[
            "src/e_pkg/data_file.json",
            "src/e_pkg/py_typed",
            "src/e_pkg/read_me.md",
            "src/e_pkg/engines/cache.py",
            "src/e_pkg/my_engines/cache.py",
            "src/e_pkg/doc_processor.py",
        ],
        "ost.single-word-module",
    )?;
    let mut names: Vec<&str> = bad.iter().filter_map(|line| line.split(' ').next()).collect();
    names.sort_unstable();
    assert_eq!(names, ["doc_processor.py", "my_engines/"], "{bad:?}");
    Ok(())
}

#[test]
fn eager_no_init_flags_it_at_every_depth_in_src_and_tests() -> Outcome {
    let bad = found(
        &[
            "src/e_pkg/__init__.py",
            "src/e_pkg/engines/__init__.py",
            "tests/unit/e_pkg/__init__.py",
            "src/e_pkg/engines/cache.py",
        ],
        "ost.no-init",
    )?;
    assert_eq!(bad.len(), 3, "{bad:?}");
    assert!(bad.iter().all(|line| line.starts_with("__init__.py ")), "{bad:?}");
    assert!(found(&["src/e_pkg/engines/cache.py"], "ost.no-init")?.is_empty());
    Ok(())
}

#[test]
fn eager_folder_vocabulary_flags_the_banned_folders_and_modules_only() -> Outcome {
    let bad = found(
        &[
            "src/e_pkg/core/run.py",
            "src/e_pkg/logic/common/run.py",
            "src/e_pkg/backends/pg.py",
            "src/e_pkg/clients/http.py",
            "src/e_pkg/gateways/pay.py",
            "src/e_pkg/engines/utils.py",
            "src/e_pkg/engines/helpers.py",
            "src/e_pkg/infra/errors.py",
            "src/e_pkg/providers/drivers/http/wreq.py",
        ],
        "ost.folder-vocabulary",
    )?;
    let mut names: Vec<&str> = bad.iter().filter_map(|line| line.split(' ').next()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "backends/",
            "clients/",
            "common/",
            "core/",
            "gateways/",
            "helpers.py",
            "utils.py"
        ],
        "{bad:?}"
    );
    Ok(())
}

#[test]
fn eager_provider_layout_wants_a_capability_folder_between_the_nature_and_the_file() -> Outcome {
    let id = "natures.provider-layout";
    let bad = found(
        &[
            "src/e_pkg/providers/drivers/psqlpy.py",
            "src/e_pkg/providers/adapters/doc.py",
            "src/e_pkg/providers/drivers/database/asyncpg.py",
            "src/e_pkg/providers/adapters/speech/piper.py",
        ],
        id,
    )?;
    let mut names: Vec<&str> = bad.iter().filter_map(|line| line.split(' ').next()).collect();
    names.sort_unstable();
    assert_eq!(names, ["doc.py", "psqlpy.py"], "{bad:?}");
    assert!(passed(
        &project(&["src/e_pkg/providers/drivers/database/asyncpg.py"])?.check(&["--select", id])?
    ));
    Ok(())
}

#[test]
fn eager_toplevel_adapters_is_banned_under_the_package_and_legal_under_providers() -> Outcome {
    let id = "ost.no-toplevel-adapters";
    let bad = found(
        &[
            "src/e_pkg/adapters/doc.py",
            "src/e_pkg/providers/adapters/speech/piper.py",
            "src/e_pkg/engines/adapters/doc.py",
        ],
        id,
    )?;
    assert_eq!(bad.len(), 1, "{bad:?}");
    assert!(bad.iter().all(|line| line.starts_with("adapters/ ")), "{bad:?}");
    assert!(found(&["src/e_pkg/providers/adapters/speech/piper.py"], id)?.is_empty());
    Ok(())
}
