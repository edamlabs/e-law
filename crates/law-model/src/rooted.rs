use std::path::{Component, Path, PathBuf};

/// What carries paths that read relative to the project root when inside it, and stay absolute outside it.
pub trait Rooted {
    /// Itself, every path inside `root` made relative to it.
    #[must_use]
    fn rooted(self, root: &Path) -> Self;
}

impl Rooted for PathBuf {
    fn rooted(self, root: &Path) -> Self {
        let inside = self
            .strip_prefix(root)
            .ok()
            .filter(|relative| {
                !relative.as_os_str().is_empty()
                    && relative.components().all(|part| matches!(part, Component::Normal(_)))
            })
            .map(Path::to_owned);
        inside.unwrap_or(self)
    }
}
