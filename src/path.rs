use std::path::{Path, PathBuf};

pub const COLLECTIONS_DIR: &str = "collections";
pub const CONTENT_DIR: &str = "root";
pub const TEMPLATES_DIR: &str = "templates";

/// Absolute path, with symlinks resolved in the part of the path that exists
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AbsPath(PathBuf);

impl AbsPath {
    pub fn inner(&self) -> &Path {
        &self.0
    }

    pub fn into_inner(self) -> PathBuf {
        self.0
    }

    /// Re-root a path living under `base_dir` into `out_dir`
    pub fn try_swap_base(&self, base_dir: &Path, out_dir: &Path) -> anyhow::Result<Self> {
        let relative = self.0.strip_prefix(base_dir)?;
        Ok(Self(out_dir.join(relative)))
    }
}

impl TryFrom<PathBuf> for AbsPath {
    type Error = anyhow::Error;

    fn try_from(value: PathBuf) -> Result<Self, Self::Error> {
        let path = std::path::absolute(value)?;
        Ok(Self(resolve_symlinks(&path)))
    }
}

/// Resolve symlinks in `path`, falling back to its parent for missing targets
///
/// Not-yet-created paths, such as a fresh output directory, must resolve to the
/// same form as existing ones, or path comparisons and `strip_prefix` fail.
fn resolve_symlinks(path: &Path) -> PathBuf {
    if let Ok(resolved) = std::fs::canonicalize(path) {
        return resolved;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => match std::fs::canonicalize(parent) {
            Ok(parent) => parent.join(name),
            Err(_) => path.to_path_buf(),
        },
        _ => path.to_path_buf(),
    }
}

impl std::fmt::Display for AbsPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.to_string_lossy())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn abs(path: &str) -> AbsPath {
        AbsPath::try_from(PathBuf::from(path)).unwrap()
    }

    #[test]
    fn swaps_base_directory_for_output_directory() {
        let file = abs("/site/root/blog/post.html");
        let swapped = file
            .try_swap_base(Path::new("/site/root"), Path::new("/build"))
            .unwrap();
        assert_eq!(swapped, abs("/build/blog/post.html"));
    }

    #[test]
    fn rejects_paths_outside_the_base_directory() {
        let file = abs("/site/root/post.html");
        assert!(
            file.try_swap_base(Path::new("/other"), Path::new("/build"))
                .is_err()
        );
    }

    #[test]
    fn resolves_missing_paths_against_their_canonical_parent() {
        let dir = tempfile::tempdir().unwrap();
        let canonical = std::fs::canonicalize(dir.path()).unwrap();
        let missing = AbsPath::try_from(canonical.join("dist")).unwrap();
        let created_afterwards = AbsPath::try_from(dir.path().join("dist")).unwrap();

        assert_eq!(missing, created_afterwards);
        assert_eq!(missing.inner().parent(), Some(canonical.as_path()));
    }
}
