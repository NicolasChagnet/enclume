use std::path::Path;

use log::warn;

use crate::path::AbsPath;

/// Find all existing files matching a glob pattern.
///
/// The `glob` crate has no brace support, so `{md,html}` alternatives are
/// expanded into separate patterns before matching.
pub fn glob_files(pattern: &Path) -> anyhow::Result<Vec<AbsPath>> {
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
                    warn!("Skipping unreadable path matching {expanded:?}: {error}");
                    continue;
                }
            };
            if path.is_file()
                && let Ok(path) = AbsPath::try_from(path)
            {
                files.push(path);
            }
        }
    }
    Ok(files)
}

/// Select the files matching a pattern relative to `base`, without touching the filesystem
pub fn match_files(base: &Path, pattern: &str, files: &[AbsPath]) -> anyhow::Result<Vec<AbsPath>> {
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
    let mut matched: Vec<AbsPath> = files
        .iter()
        .filter(|file| {
            file.inner().strip_prefix(base).is_ok_and(|relative| {
                patterns
                    .iter()
                    .any(|pattern| pattern.matches_path_with(relative, options))
            })
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
    fn matches_files_relative_to_a_base_directory() {
        let files = vec![
            AbsPath::try_from(std::path::PathBuf::from("/site/root/blog/post.html")).unwrap(),
            AbsPath::try_from(std::path::PathBuf::from("/site/root/blog/post.md")).unwrap(),
            AbsPath::try_from(std::path::PathBuf::from("/site/root/other.html")).unwrap(),
        ];
        let matched = match_files(Path::new("/site/root"), "blog/**/*.{md,html}", &files).unwrap();
        assert_eq!(matched.len(), 2);
        assert!(
            matched
                .iter()
                .all(|file| file.inner().starts_with("/site/root/blog"))
        );
    }
}
