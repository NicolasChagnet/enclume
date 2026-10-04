use std::path::{Path, PathBuf};

use crate::path::SitePath;

/// Find all existing files matching a glob pattern.
///
/// The `glob` crate has no brace support, so `{md,html}` alternatives are
/// expanded into separate patterns before matching.
pub fn glob_files(pattern: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let Some(pattern) = pattern.to_str() else {
        anyhow::bail!("Glob pattern {:?} is not valid UTF-8", pattern);
    };
    let mut files = Vec::new();
    for expanded in expand_braces(pattern) {
        let paths = glob::glob(&expanded)
            .map_err(|e| anyhow::anyhow!("Invalid glob pattern {:?}: {e}", expanded))?;
        for entry in paths {
            let path = match entry {
                Ok(path) => path,
                Err(error) => {
                    log::warn!("Skipping unreadable path matching {expanded:?}: {error}");
                    continue;
                }
            };
            if path.is_file() {
                files.push(path);
            }
        }
    }
    Ok(files)
}

/// Select the files matching a pattern relative to a site root, without touching the filesystem
pub fn match_files(pattern: &str, files: &[SitePath]) -> anyhow::Result<Vec<SitePath>> {
    let patterns = expand_braces(pattern)
        .into_iter()
        .map(|p| {
            glob::Pattern::new(&p).map_err(|e| anyhow::anyhow!("Invalid glob pattern {p:?}: {e}"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    // `require_literal_separator` keeps `*` from crossing `/`, which matches
    // what a filesystem walk would return
    let options = glob::MatchOptions {
        require_literal_separator: true,
        ..Default::default()
    };
    let mut matched: Vec<SitePath> = files
        .iter()
        .filter(|file| {
            patterns
                .iter()
                .any(|pattern| pattern.matches_path_with(file.as_path(), options))
        })
        .cloned()
        .collect();
    matched.sort();
    Ok(matched)
}

/// Expand `{a,b}` alternatives into separate patterns
fn expand_braces(pattern: &str) -> Vec<String> {
    let Some(open) = pattern.find('{') else {
        return vec![pattern.to_string()];
    };
    let Some(close) = pattern[open..].find('}').map(|i| open + i) else {
        return vec![pattern.to_string()];
    };
    pattern[open + 1..close]
        .split(',')
        .flat_map(|alternative| {
            let expanded = format!(
                "{}{}{}",
                &pattern[..open],
                alternative,
                &pattern[close + 1..]
            );
            expand_braces(&expanded)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_brace_alternatives() {
        assert_eq!(expand_braces("blog/**/*.md"), vec!["blog/**/*.md"]);
        assert_eq!(
            expand_braces("blog/**/*.{md,html}"),
            vec!["blog/**/*.md", "blog/**/*.html"]
        );
        assert_eq!(
            expand_braces("{a,b}/{c,d}.html"),
            vec!["a/c.html", "a/d.html", "b/c.html", "b/d.html"]
        );
    }

    #[test]
    fn matches_files_relative_to_a_site_root() {
        let files: Vec<SitePath> = ["blog/post.html", "blog/post.md", "other.html"]
            .iter()
            .map(|path| SitePath::try_from(PathBuf::from(path)).unwrap())
            .collect();

        let matched = match_files("blog/**/*.{md,html}", &files).unwrap();

        assert_eq!(matched.len(), 2);
        assert!(
            matched
                .iter()
                .all(|file| file.as_path().starts_with("blog"))
        );
    }
}
