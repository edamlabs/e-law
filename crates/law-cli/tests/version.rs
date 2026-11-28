use std::process::Command;

#[test]
fn version_prints_name_and_semver() -> Result<(), Box<dyn std::error::Error>> {
    let out = Command::new(env!("CARGO_BIN_EXE_law")).arg("--version").output()?;
    assert_eq!(
        String::from_utf8(out.stdout)?,
        format!("law {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(String::from_utf8(out.stderr)?, "");
    assert_eq!(out.status.code(), Some(0));
    Ok(())
}

#[test]
fn bare_law_prints_help_and_exits_2() -> Result<(), Box<dyn std::error::Error>> {
    let out = Command::new(env!("CARGO_BIN_EXE_law")).output()?;
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(String::from_utf8(out.stdout)?, "");
    let help = String::from_utf8(out.stderr)?;
    assert!(help.contains("Usage: law [OPTIONS] <COMMAND>"), "{help}");
    assert!(help.contains("check"), "{help}");
    Ok(())
}
