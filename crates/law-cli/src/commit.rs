use std::path::Path;
use std::process::{Command, Stdio};

const GIT: &str = "git";
const HEAD: [&str; 2] = ["rev-parse", "HEAD"];

/// The commit a project's checkout stands on.
pub struct Commit;

impl Commit {
    /// The commit `git rev-parse HEAD` names at a root; none when git is absent or the root is in no repository.
    pub fn head(root: &Path) -> Option<String> {
        let out = Command::new(GIT)
            .args(HEAD)
            .current_dir(root)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        let commit = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        (out.status.success() && !commit.is_empty()).then_some(commit)
    }
}
