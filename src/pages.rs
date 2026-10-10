use snafu::prelude::*;

use crate::{
    content::ParsedContent, glob::expand_braces, output::OutputLayout, parsers::VarMap,
    path::SitePath,
};

/// A page a template can link to
///
/// Built from every content file which declared metadata. Files the build copies
/// verbatim are not pages.
#[derive(Debug, Clone)]
pub struct Page {
    path: SitePath,
    url: String,
    vars: VarMap,
}

impl Page {
    /// The path of the source file, relative to the content root
    pub fn path(&self) -> &SitePath {
        &self.path
    }

    /// The URL the page is served at
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The variables the file declared under `vars`
    pub fn vars(&self) -> &VarMap {
        &self.vars
    }
}

/// Every page of the build, behind the `pages()` template function
///
/// Templates see this instead of the whole content tree, so a page can only read
/// the metadata its siblings declare in their frontmatter.
#[derive(Debug, Clone, Default)]
pub struct Pages {
    pages: Vec<Page>,
}

impl Pages {
    /// Collect the pages among the parsed content files
    ///
    /// Kept in source-path order, so a listing which does not sort is still
    /// deterministic across builds.
    pub fn collect<'a>(
        parsed: impl IntoIterator<Item = &'a ParsedContent>,
        layout: &OutputLayout,
    ) -> Result<Self, snafu::Whatever> {
        let mut pages = Vec::new();
        for content in parsed {
            let path = content.path().clone();
            pages.push(Page {
                url: layout.page_url(&path)?,
                path,
                vars: content.metadata().vars().clone(),
            });
        }
        pages.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(Self { pages })
    }

    /// Every page, in source-path order
    pub fn all(&self) -> &[Page] {
        &self.pages
    }

    /// The pages whose source path matches a glob pattern
    ///
    /// `{a,b}` alternatives are expanded like the build's own file patterns, and
    /// `*` does not cross a directory separator, so `blog/*.md` skips
    /// `blog/nested/post.md`.
    pub fn matching(&self, pattern: &str) -> Result<Vec<&Page>, snafu::Whatever> {
        let matchers = expand_braces(pattern)
            .into_iter()
            .map(|pattern| {
                glob::Pattern::new(&pattern)
                    .with_whatever_context(|error| format!("Invalid pattern {pattern:?}: {error}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let options = glob::MatchOptions {
            require_literal_separator: true,
            ..Default::default()
        };
        Ok(self
            .pages
            .iter()
            .filter(|page| {
                matchers
                    .iter()
                    .any(|matcher| matcher.matches_path_with(page.path.as_path(), options))
            })
            .collect())
    }

    /// The pages matching `pattern`, shaped for the `pages()` template function
    ///
    /// Each page is `{path, url, vars}`. Variables keep their JSON types, so a
    /// template can read numbers, booleans and lists.
    pub fn as_value(&self, pattern: &str) -> Result<serde_json::Value, snafu::Whatever> {
        let pages = self
            .matching(pattern)?
            .into_iter()
            .map(|page| {
                serde_json::json!({
                    "path": page.path.as_str(),
                    "url": page.url,
                    "vars": page.vars,
                })
            })
            .collect();
        Ok(serde_json::Value::Array(pages))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{content::ContentFile, parsers::ParsedData, path::Roots};
    use std::path::PathBuf;

    fn site_path(path: &str) -> SitePath {
        SitePath::try_from(PathBuf::from(path)).unwrap()
    }

    /// A content root holding no file: locating only computes paths
    fn roots() -> (tempfile::TempDir, Roots) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        let roots = Roots::new(dir.path().join("src"), dir.path().join("dist")).unwrap();
        (dir, roots)
    }

    /// A parsed page declaring only a title
    fn page(roots: &Roots, path: &str, title: &str) -> ParsedContent {
        let file = ContentFile::locate(roots, site_path(path)).unwrap();
        let mut vars = VarMap::new();
        vars.insert("title".to_string(), serde_json::json!(title));
        let data = ParsedData::new(site_path("post.html"), vars, serde_json::json!(""));
        ParsedContent::new(file, data)
    }

    #[test]
    fn collects_pages_with_the_url_they_are_served_at() {
        let (_dir, roots) = roots();
        let pages = Pages::collect(
            &[
                page(&roots, "index.md", "Home"),
                page(&roots, "blog/index.md", "Blog"),
                page(&roots, "blog/post.md", "Post"),
                page(&roots, "404.md", "404"),
            ],
            &OutputLayout::default(),
        )
        .unwrap();

        let urls: Vec<_> = pages.all().iter().map(|page| page.url()).collect();
        // Source-path order: `404.md`, `blog/index.md`, `blog/post.md`, `index.md`
        assert_eq!(urls, ["/404.html", "/blog", "/blog/post", "/"]);
    }

    #[test]
    fn selects_the_pages_matching_a_pattern() {
        let (_dir, roots) = roots();
        let pages = Pages::collect(
            &[
                page(&roots, "blog/index.md", "Blog"),
                page(&roots, "blog/post.md", "Post"),
                page(&roots, "blog/nested/deep.md", "Deep"),
                page(&roots, "about.md", "About"),
            ],
            &OutputLayout::default(),
        )
        .unwrap();

        let titles = |pattern| {
            pages
                .matching(pattern)
                .unwrap()
                .iter()
                .map(|page| page.vars()["title"].as_str().unwrap().to_string())
                .collect::<Vec<_>>()
        };

        // `*` does not cross a directory separator
        assert_eq!(titles("blog/*.md"), ["Blog", "Post"]);
        assert_eq!(titles("blog/**/*.md"), ["Blog", "Deep", "Post"]);
        // Braces expand like the build's own patterns
        assert_eq!(titles("blog/**/*.{md,html}"), ["Blog", "Deep", "Post"]);
        assert!(titles("nothing/**/*.md").is_empty());
    }

    #[test]
    fn rejects_an_invalid_pattern() {
        let (_dir, roots) = roots();
        let post = page(&roots, "blog/post.md", "Post");
        let pages = Pages::collect([&post], &OutputLayout::default()).unwrap();

        assert!(pages.matching("blog/[").is_err());
    }

    #[test]
    fn exposes_each_page_as_path_url_and_vars() {
        let (_dir, roots) = roots();
        let post = page(&roots, "blog/post.md", "Post");
        let pages = Pages::collect([&post], &OutputLayout::default()).unwrap();

        let value = pages.as_value("**/*.md").unwrap();
        assert_eq!(value[0]["path"], "blog/post.md");
        assert_eq!(value[0]["url"], "/blog/post");
        assert_eq!(value[0]["vars"]["title"], "Post");
    }
}
