use std::path::{Component, Path, PathBuf};

use anyhow::Context;

pub const COLLECTIONS_DIR: &str = "collections";
pub const CONTENT_DIR: &str = "root";
pub const TEMPLATES_DIR: &str = "templates";

/// The directory roots of a site
///
/// `base` and `out` are absolute with symlinks resolved in the part that
/// exists. Not-yet-created paths, such as a fresh output directory, must
/// resolve to the same form as existing ones, or prefix checks and the
/// deletion guards fail. Every path below these roots is a [`SitePath`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Roots {
    base: PathBuf,
    out: PathBuf,
}

impl Roots {
    /// Resolve both roots, refusing overlapping trees before anything is deleted
    pub fn new(base: PathBuf, out: PathBuf) -> anyhow::Result<Self> {
        let base = std::path::absolute(&base)?;
        let base = std::fs::canonicalize(&base)
            .with_context(|| format!("Base directory {} does not exist", base.display()))?;
        let out = resolve_existing_prefix(&std::path::absolute(out)?);
        let roots = Self { base, out };
        if roots.base.starts_with(&roots.out) || roots.out.starts_with(&roots.base) {
            anyhow::bail!(
                "Output directory {} overlaps base directory {}",
                roots.out.display(),
                roots.base.display()
            );
        }
        Ok(roots)
    }

    pub fn base(&self) -> &Path {
        &self.base
    }

    pub fn out(&self) -> &Path {
        &self.out
    }

    pub fn content_dir(&self) -> PathBuf {
        self.base.join(CONTENT_DIR)
    }

    pub fn templates_dir(&self) -> PathBuf {
        self.base.join(TEMPLATES_DIR)
    }

    pub fn collections_dir(&self) -> PathBuf {
        self.base.join(COLLECTIONS_DIR)
    }
}

/// Resolve symlinks in the part of `path` that exists, keeping any missing tail
fn resolve_existing_prefix(path: &Path) -> PathBuf {
    let mut candidate = path;
    let mut tail: Vec<&std::ffi::OsStr> = Vec::new();
    loop {
        if let Ok(mut resolved) = std::fs::canonicalize(candidate) {
            for name in tail.iter().rev() {
                resolved.push(*name);
            }
            return resolved;
        }
        match (candidate.parent(), candidate.file_name()) {
            (Some(parent), Some(name)) => {
                tail.push(name);
                candidate = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// A path relative to a site root
///
/// Rejects absolute paths and `..`, so a content file name can never read or
/// write outside its root.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SitePath(PathBuf);

impl SitePath {
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Change the extension of the underlying path
    pub fn with_extension(self, extension: &str) -> Self {
        Self(self.0.with_extension(extension))
    }
}

impl TryFrom<PathBuf> for SitePath {
    type Error = anyhow::Error;

    fn try_from(value: PathBuf) -> Result<Self, Self::Error> {
        let valid = !value.as_os_str().is_empty()
            && value
                .components()
                .all(|component| matches!(component, Component::Normal(_)));
        if !valid {
            anyhow::bail!("Not a site-relative path: {}", value.display());
        }
        Ok(Self(value))
    }
}

impl std::fmt::Display for SitePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.to_string_lossy())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site_path(path: &str) -> anyhow::Result<SitePath> {
        SitePath::try_from(PathBuf::from(path))
    }

    #[test]
    fn resolves_a_missing_out_under_its_existing_parent() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();

        let roots = Roots::new(dir.path().join("src"), dir.path().join("dist")).unwrap();

        let canonical = std::fs::canonicalize(dir.path()).unwrap();
        assert_eq!(roots.base(), canonical.join("src"));
        assert_eq!(roots.out(), canonical.join("dist"));
    }

    #[test]
    fn rejects_a_missing_base_directory() {
        let dir = tempfile::tempdir().unwrap();

        assert!(Roots::new(dir.path().join("src"), dir.path().join("dist")).is_err());
    }

    #[test]
    fn rejects_overlapping_roots() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("src");
        std::fs::create_dir(&base).unwrap();

        assert!(Roots::new(base.clone(), base.join("dist")).is_err());
        assert!(Roots::new(base.clone(), base).is_err());
    }

    #[test]
    fn rejects_paths_that_escape_their_root() {
        for path in ["/etc/passwd", "../secret", "blog/../../secret", ""] {
            assert!(site_path(path).is_err(), "{path:?} was accepted");
        }
    }

    #[test]
    fn keeps_nested_relative_paths() {
        assert_eq!(
            site_path("blog/post.md").unwrap().as_path(),
            Path::new("blog/post.md")
        );
    }
}
