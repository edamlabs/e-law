use std::fs;
use std::path::{Path, PathBuf};

use law_engine::Environment;

type Outcome = Result<(), Box<dyn std::error::Error>>;

fn dir(root: &Path, relative: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let path = root.join(relative);
    fs::create_dir_all(&path)?;
    Ok(fs::canonicalize(path)?)
}

fn config(root: &Path, text: &str) -> Outcome {
    dir(root, ".venv")?;
    fs::write(root.join(".venv/pyvenv.cfg"), text)?;
    Ok(())
}

fn home(root: &Path) -> String {
    format!("home = {}/py/bin\n", root.display())
}

#[test]
fn environment_without_a_venv_is_empty() -> Outcome {
    let tmp = tempfile::tempdir()?;
    let environment = Environment::discover(tmp.path());
    assert_eq!(environment, Environment::default());
    assert!(environment.venv.is_none() && environment.stdlib.is_none() && environment.roots.is_empty());
    assert_eq!(environment.to_string(), "no .venv, no foreign roots");
    Ok(())
}

#[test]
fn environment_a_venv_file_instead_of_a_directory_is_no_venv() -> Outcome {
    let tmp = tempfile::tempdir()?;
    fs::write(tmp.path().join(".venv"), "")?;
    assert_eq!(Environment::discover(tmp.path()), Environment::default());
    Ok(())
}

#[test]
fn environment_a_venv_without_a_config_has_sites_and_no_stdlib() -> Outcome {
    let tmp = tempfile::tempdir()?;
    let site = dir(tmp.path(), ".venv/lib/python3.14/site-packages")?;
    let environment = Environment::discover(tmp.path());
    assert_eq!(environment.venv, Some(tmp.path().join(".venv")));
    assert_eq!(environment.stdlib, None);
    assert_eq!(environment.roots, [site]);
    assert_eq!(
        environment.to_string(),
        format!(
            "{}: stdlib no stdlib (pyvenv.cfg names none); 1 foreign roots",
            tmp.path().join(".venv").display()
        )
    );
    Ok(())
}

#[test]
fn environment_an_empty_venv_has_nothing_to_search() -> Outcome {
    let tmp = tempfile::tempdir()?;
    dir(tmp.path(), ".venv")?;
    let environment = Environment::discover(tmp.path());
    assert_eq!(environment.venv, Some(tmp.path().join(".venv")));
    assert_eq!((environment.stdlib, environment.roots), (None, vec![]));
    Ok(())
}

#[test]
fn environment_version_info_key_names_the_stdlib() -> Outcome {
    let tmp = tempfile::tempdir()?;
    let stdlib = dir(tmp.path(), "py/lib/python3.14")?;
    let site = dir(tmp.path(), ".venv/lib/python3.14/site-packages")?;
    config(
        tmp.path(),
        &format!("{}version_info = 3.14.0.final.0\n", home(tmp.path())),
    )?;
    let environment = Environment::discover(tmp.path());
    assert_eq!(environment.stdlib, Some(stdlib.clone()));
    assert_eq!(environment.roots, [stdlib, site]);
    Ok(())
}

#[test]
fn environment_version_key_names_the_stdlib() -> Outcome {
    let tmp = tempfile::tempdir()?;
    let stdlib = dir(tmp.path(), "py/lib/python3.13")?;
    dir(tmp.path(), ".venv/lib/python3.13/site-packages")?;
    config(tmp.path(), &format!("{}version = 3.13.2\n", home(tmp.path())))?;
    assert_eq!(Environment::discover(tmp.path()).stdlib, Some(stdlib));
    Ok(())
}

#[test]
fn environment_version_info_wins_over_version() -> Outcome {
    let tmp = tempfile::tempdir()?;
    let stdlib = dir(tmp.path(), "py/lib/python3.14")?;
    dir(tmp.path(), "py/lib/python3.13")?;
    dir(tmp.path(), ".venv/lib/python3.14/site-packages")?;
    config(
        tmp.path(),
        &format!("{}version = 3.13.2\nversion_info = 3.14.0.final.0\n", home(tmp.path())),
    )?;
    assert_eq!(Environment::discover(tmp.path()).stdlib, Some(stdlib));
    Ok(())
}

#[test]
fn environment_only_the_site_directory_name_gives_the_version() -> Outcome {
    let tmp = tempfile::tempdir()?;
    let stdlib = dir(tmp.path(), "py/lib/python3.12")?;
    let site = dir(tmp.path(), ".venv/lib/python3.12/site-packages")?;
    config(tmp.path(), &home(tmp.path()))?;
    let environment = Environment::discover(tmp.path());
    assert_eq!(environment.stdlib, Some(stdlib.clone()));
    assert_eq!(environment.roots, [stdlib, site]);
    Ok(())
}

#[test]
fn environment_no_version_anywhere_leaves_no_stdlib() -> Outcome {
    let tmp = tempfile::tempdir()?;
    dir(tmp.path(), "py/lib/python3.12")?;
    config(tmp.path(), &home(tmp.path()))?;
    let environment = Environment::discover(tmp.path());
    assert_eq!((environment.stdlib, environment.roots), (None, vec![]));
    Ok(())
}

#[test]
fn environment_a_config_without_home_has_no_stdlib_but_keeps_its_sites() -> Outcome {
    let tmp = tempfile::tempdir()?;
    dir(tmp.path(), "py/lib/python3.14")?;
    let site = dir(tmp.path(), ".venv/lib/python3.14/site-packages")?;
    config(tmp.path(), "version_info = 3.14.0.final.0\n")?;
    let environment = Environment::discover(tmp.path());
    assert_eq!((environment.stdlib, environment.roots), (None, vec![site]));
    Ok(())
}

#[test]
fn environment_a_stdlib_directory_that_does_not_exist_is_none() -> Outcome {
    let tmp = tempfile::tempdir()?;
    let site = dir(tmp.path(), ".venv/lib/python3.14/site-packages")?;
    config(
        tmp.path(),
        &format!("{}version_info = 3.14.0.final.0\n", home(tmp.path())),
    )?;
    let environment = Environment::discover(tmp.path());
    assert_eq!((environment.stdlib, environment.roots), (None, vec![site]));
    Ok(())
}

#[test]
fn environment_config_keys_are_trimmed_and_the_first_equals_splits() -> Outcome {
    let tmp = tempfile::tempdir()?;
    let stdlib = dir(tmp.path(), "py/lib/python3.14")?;
    dir(tmp.path(), ".venv/lib/python3.14/site-packages")?;
    config(
        tmp.path(),
        &format!(
            "include-system-site-packages = false\n  home   =   {}/py/bin  \nversion_info=3.14.0\n",
            tmp.path().display()
        ),
    )?;
    assert_eq!(Environment::discover(tmp.path()).stdlib, Some(stdlib));
    Ok(())
}

#[test]
fn environment_roots_are_stdlib_then_its_dynload_then_site_packages() -> Outcome {
    let tmp = tempfile::tempdir()?;
    let stdlib = dir(tmp.path(), "py/lib/python3.14")?;
    let dynload = dir(tmp.path(), "py/lib/python3.14/lib-dynload")?;
    let site = dir(tmp.path(), ".venv/lib/python3.14/site-packages")?;
    config(tmp.path(), &format!("{}version_info = 3.14.0\n", home(tmp.path())))?;
    assert_eq!(Environment::discover(tmp.path()).roots, [stdlib, dynload, site]);
    Ok(())
}

#[test]
fn environment_lib64_sites_are_read_and_a_link_to_lib_is_one_root() -> Outcome {
    let tmp = tempfile::tempdir()?;
    let site = dir(tmp.path(), ".venv/lib/python3.14/site-packages")?;
    std::os::unix::fs::symlink("lib", tmp.path().join(".venv/lib64"))?;
    let environment = Environment::discover(tmp.path());
    assert_eq!(environment.roots, [site]);
    Ok(())
}

#[test]
fn environment_lib64_alone_is_read() -> Outcome {
    let tmp = tempfile::tempdir()?;
    let site = dir(tmp.path(), ".venv/lib64/python3.14/site-packages")?;
    assert_eq!(Environment::discover(tmp.path()).roots, [site]);
    Ok(())
}

#[test]
fn environment_several_pythons_give_every_site_sorted() -> Outcome {
    let tmp = tempfile::tempdir()?;
    let new = dir(tmp.path(), ".venv/lib/python3.14/site-packages")?;
    let old = dir(tmp.path(), ".venv/lib/python3.12/site-packages")?;
    assert_eq!(Environment::discover(tmp.path()).roots, [old, new]);
    Ok(())
}

#[test]
fn environment_only_python_directories_without_site_packages_add_no_root() -> Outcome {
    let tmp = tempfile::tempdir()?;
    dir(tmp.path(), ".venv/lib/python3.14")?;
    dir(tmp.path(), ".venv/lib/other/site-packages")?;
    assert!(Environment::discover(tmp.path()).roots.is_empty());
    Ok(())
}

#[test]
fn environment_display_says_the_venv_the_stdlib_and_how_many_roots() -> Outcome {
    let tmp = tempfile::tempdir()?;
    let stdlib = dir(tmp.path(), "py/lib/python3.14")?;
    dir(tmp.path(), ".venv/lib/python3.14/site-packages")?;
    config(tmp.path(), &format!("{}version_info = 3.14.0\n", home(tmp.path())))?;
    assert_eq!(
        Environment::discover(tmp.path()).to_string(),
        format!(
            "{}: stdlib {}; 2 foreign roots",
            tmp.path().join(".venv").display(),
            stdlib.display()
        )
    );
    Ok(())
}
