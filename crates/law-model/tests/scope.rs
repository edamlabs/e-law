use law_model::{Glob, Scope};

type Outcome = Result<(), Box<dyn std::error::Error>>;

fn scope(include: &str, exclude: &[&str]) -> Result<Scope, Box<dyn std::error::Error>> {
    Ok(Scope {
        include: vec![include.parse()?],
        exclude: exclude
            .iter()
            .map(|text| text.parse::<Glob>())
            .collect::<Result<_, _>>()?,
        kind: None,
    })
}

fn sound(include: &str) -> Outcome {
    let scope = scope(include, &["**/ops/**", "**/__pycache__/**"])?;
    let sample = scope.sample();
    assert!(
        scope.include.iter().all(|glob| glob.matches(&sample)),
        "include {include:?}: sample {} is not matched by it",
        sample.display()
    );
    assert!(
        scope.exclude.iter().all(|glob| !glob.matches(&sample)),
        "include {include:?}: sample {} is excluded",
        sample.display()
    );
    Ok(())
}

#[test]
fn scope_default_includes_python_everywhere() {
    let scope = Scope::default();
    assert_eq!(scope.include.len(), 1);
    assert!(scope.include.iter().all(|glob| glob.as_str() == "**/*.py"));
    assert!(scope.exclude.is_empty());
    assert!(scope.kind.is_none());
}

#[test]
fn scope_sample_src_doublestar() -> Outcome {
    sound("src/**/*.py")
}

#[test]
fn scope_sample_any_python() -> Outcome {
    sound("**/*.py")
}

#[test]
fn scope_sample_star_dir_then_doublestar() -> Outcome {
    sound("src/*/logic/**/*.py")
}

#[test]
fn scope_sample_test_prefix() -> Outcome {
    sound("tests/**/test_*.py")
}

#[test]
fn scope_sample_braces() -> Outcome {
    sound("src/{a,b}/*.py")
}

#[test]
fn scope_sample_class() -> Outcome {
    sound("src/[ab]*/x.py")
}

#[test]
fn scope_sample_question_mark() -> Outcome {
    sound("src/m?dule/*.py")
}

#[test]
fn scope_sample_without_include_is_a_python_file() {
    let scope = Scope {
        include: Vec::new(),
        exclude: Vec::new(),
        kind: None,
    };
    assert_eq!(scope.sample(), std::path::PathBuf::from("case.py"));
}

#[test]
fn scope_refuses_unknown_field() {
    assert!(serde_json::from_str::<Scope>(r#"{"include":["a/*.py"],"inlcude":[]}"#).is_err());
}

#[test]
fn scope_defaults_when_empty() -> Outcome {
    let scope: Scope = serde_json::from_str("{}")?;
    assert_eq!(scope, Scope::default());
    Ok(())
}
