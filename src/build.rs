use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use rayon::prelude::*;
use snafu::prelude::*;

use crate::{
    content::{Content, ContentFile, ParsedContent},
    glob::glob_relative,
    output::{Assets, Output, OutputLayout},
    pages::Pages,
    parsers::{MarkdownParserOptions, Parsers},
    path::{Roots, SitePath},
    templates::TeraTemplater,
};

#[derive(Debug, Clone, Copy)]
pub(crate) enum BuildMode {
    Prod,
    Dev,
}

pub trait Builder {
    /// Build the site from `base` into `out`
    ///
    /// Renders every file which declares metadata in its frontmatter through
    /// the template it names, and copies the files without frontmatter as-is.
    /// `out` is replaced only after every file was written, so a failed build
    /// leaves the previous output untouched. Aborts on the first failure.
    fn build(&self) -> Result<(), snafu::Whatever>;
}

#[derive(Debug, Clone)]
pub struct SiteBuilder {
    roots: Roots,
    layout: OutputLayout,
    markdown_options: MarkdownParserOptions,
    mode: BuildMode,
}

impl SiteBuilder {
    pub fn new(
        roots: Roots,
        layout: OutputLayout,
        markdown_options: MarkdownParserOptions,
        mode: BuildMode,
    ) -> Self {
        Self {
            roots,
            layout,
            markdown_options,
            mode,
        }
    }

    /// Build every file into `out`
    fn build_into(&self, out: &Path) -> Result<(), snafu::Whatever> {
        reset_dir(out)?;

        let roots = &self.roots;
        let draft_pattern = match self.mode {
            BuildMode::Prod => Some(".draft"),
            BuildMode::Dev => None,
        };
        let files = glob_relative(&roots.content_dir(), "**/*", draft_pattern)?;

        // Fingerprint the assets first: templates link to them while they render
        // Wrap in Arc for the templater so 'assets' can be used within the template functions
        let assets = Arc::new(scan_assets(roots, &files, &self.layout)?);
        let output = Output::new(out, &self.layout, &assets);

        // Parsing runs first, so the `pages()` template function can list every page
        // while the templates render. Then each file is written: the ones declaring
        // metadata through their template, the rest copied verbatim.

        let parsed = self.parse_all(roots, files)?;
        let pages = Arc::new(Pages::collect(
            parsed.iter().filter_map(|(_, parsed)| parsed.as_ref()),
            &self.layout,
        )?);

        let mut templater = TeraTemplater::new(Arc::clone(&assets), Arc::clone(&pages));
        templater.load_templates(&roots.templates_dir())?;

        parsed.into_par_iter().try_for_each(|(file, parsed)| {
            // Files which declare no metadata are copied verbatim
            let Some(parsed) = parsed else {
                return file.write(&output);
            };
            let url = self.layout.page_url(parsed.path())?;
            parsed
                .render(&templater, &url)
                .with_whatever_context(|_| format!("Could not render {file}"))?
                .write(&output)
        })
    }

    /// Read and parse every content file, keeping the ones to copy verbatim
    fn parse_all(
        &self,
        roots: &Roots,
        files: Vec<SitePath>,
    ) -> Result<Vec<(ContentFile, Option<ParsedContent>)>, snafu::Whatever> {
        let parsers = Parsers::new(self.markdown_options.clone());
        files
            .into_par_iter()
            .map(|path| {
                let file = ContentFile::locate(roots, path)?;
                log::debug!("Handling {file}");
                let parsed = file
                    .parse(&parsers)
                    .with_whatever_context(|_| format!("Could not parse {file}"))?;
                Ok((file, parsed))
            })
            .collect()
    }

    /// Sibling directory the build writes into before replacing `out`
    fn staging_dir(&self) -> PathBuf {
        let mut name = self.roots.out().as_os_str().to_os_string();
        name.push(".staging");
        PathBuf::from(name)
    }
}

impl Builder for SiteBuilder {
    fn build(&self) -> Result<(), snafu::Whatever> {
        log::info!("Building website...");
        // Build next to `out` so the swap stays on one filesystem
        let staging = self.staging_dir();
        if let Err(error) = self.build_into(&staging) {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(error);
        }
        replace_dir(&staging, self.roots.out())
    }
}

/// Map every content file to its built name, fingerprinting the assets on the way
///
/// Templates read the same table through `asset()`, so a link and the file the
/// build writes always agree.
fn scan_assets(
    roots: &Roots,
    files: &[SitePath],
    layout: &OutputLayout,
) -> Result<Assets, snafu::Whatever> {
    let mut assets = Assets::default();
    for path in files {
        let file = ContentFile::locate(roots, path.clone())?;
        let digest = layout
            .fingerprints(path)
            .then(|| file.fingerprint())
            .transpose()?;
        assets.insert(path.clone(), layout.copied_file(path, digest.as_deref()));
    }
    Ok(assets)
}

/// Replace `out` with the freshly built `staging` directory
fn replace_dir(staging: &Path, out: &Path) -> Result<(), snafu::Whatever> {
    ensure_deletable(out)?;
    match std::fs::remove_dir_all(out) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => snafu::whatever!("Failed to delete {:?} (error: {e})", out),
    }
    std::fs::rename(staging, out)
        .whatever_context(format!("Couldn't rename {staging:?} into {out:?}"))?;
    Ok(())
}

/// Delete `dir` and recreate it empty
///
/// Refuses directories which hold user data, such as the working directory
fn reset_dir(dir: &Path) -> Result<(), snafu::Whatever> {
    ensure_deletable(dir)?;
    match std::fs::remove_dir_all(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => snafu::whatever!("Failed to delete {:?} (error: {e})", dir),
    }
    std::fs::create_dir_all(dir).whatever_context(format!("Couldn't create directory {dir:?}"))?;
    Ok(())
}

/// Refuse deletion of directories which hold user data
fn ensure_deletable(dir: &Path) -> Result<(), snafu::Whatever> {
    let cwd = std::env::current_dir().whatever_context("Couldn't access current directory")?;
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if dir.parent().is_none() || dir == cwd.as_path() || home.as_deref() == Some(dir) {
        whatever!(
            "Refusing to delete {}: it is a root, home, or working directory",
            dir.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use snafu::Report;
    use std::fs;

    struct Site {
        // Held to remove the temporary directory once the test ends
        _dir: tempfile::TempDir,
        roots: Roots,
        layout: OutputLayout,
        mode: BuildMode,
    }

    impl Site {
        fn new() -> Self {
            Self::with_layout(OutputLayout::default())
        }

        fn with_layout(layout: OutputLayout) -> Self {
            let dir = tempfile::tempdir().unwrap();
            fs::create_dir(dir.path().join("src")).unwrap();
            let roots = Roots::new(dir.path().join("src"), dir.path().join("dist")).unwrap();
            Self {
                roots,
                layout,
                mode: BuildMode::Prod,
                _dir: dir,
            }
        }

        /// Build the way `enclume dev` does
        fn with_mode(mut self, mode: BuildMode) -> Self {
            self.mode = mode;
            self
        }

        fn write(&self, relative: &str, content: &str) {
            let path = self.roots.base().join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }

        fn read(&self, relative: &str) -> String {
            fs::read_to_string(self.roots.out().join(relative)).unwrap()
        }

        fn exists(&self, relative: &str) -> bool {
            self.roots.out().join(relative).exists()
        }

        fn build(&self) -> Result<(), snafu::Whatever> {
            self.build_with(MarkdownParserOptions::default())
        }

        fn build_with(
            &self,
            markdown_options: MarkdownParserOptions,
        ) -> Result<(), snafu::Whatever> {
            SiteBuilder::new(self.roots.clone(), self.layout, markdown_options, self.mode).build()
        }
    }

    /// The link a page points at, read out of an `href` attribute
    fn href(page: &str) -> &str {
        page.split("href=\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .unwrap_or_else(|| panic!("no link in page: {page}"))
    }

    #[test]
    fn renders_content_files_and_copies_other_files() {
        let site = Site::new();
        site.write(
            "templates/post.html",
            "<h1>{{ vars.title | default(value=\"\") }}</h1>{{__content__}}",
        );
        site.write(
            "root/blog/post.md",
            "---\ntemplate: post.html\nvars:\n  title: Hello <b>world</b>\n---\nbody\n",
        );
        site.write(
            "root/blog/about.html",
            "---\ntemplate: post.html\nvars:\n  title: About <i>us</i>\n---\n<p>raw *html*</p>\n",
        );
        site.write("root/blog/other.html", "<p>no frontmatter</p>\n");
        site.write("root/assets/style.css", "body {}\n");

        site.build().unwrap();

        let page = site.read("blog/post/index.html");
        // Metadata is escaped, the parsed content is inserted as raw HTML
        assert!(page.contains("<h1>Hello &lt;b&gt;world&lt;/b&gt;</h1>"));
        assert!(page.contains("<p>body</p>"));
        let page = site.read("blog/about/index.html");
        assert!(page.contains("<h1>About &lt;i&gt;us&lt;/i&gt;</h1>"));
        // The HTML body passes through verbatim, without Markdown conversion
        assert!(page.contains("<p>raw *html*</p>"));
        assert!(
            site.read("blog/other.html")
                .contains("<p>no frontmatter</p>")
        );
        assert_eq!(site.read("assets/style.css"), "body {}\n");
        assert!(!site.exists("root"));
        assert!(!site.exists("blog/post.md"));
    }

    #[test]
    fn renders_templates_which_include_other_templates() {
        let site = Site::new();
        site.write("templates/layout.html", "<main>{{__content__}}</main>");
        site.write("templates/post.html", "{% include \"layout.html\" %}");
        site.write(
            "root/blog/post.md",
            "---\ntemplate: post.html\nvars:\n  title: Hello\n---\n# body\n",
        );

        site.build().unwrap();

        let page = site.read("blog/post/index.html");
        assert!(page.contains("<main>"));
        assert!(page.contains("<h1 id=\"body\">"));
    }

    #[test]
    fn renders_json_content_files() {
        let site = Site::new();
        site.write(
            "templates/post.html",
            "<h1>{{ vars.title }}</h1><span>{{ vars.views }}</span>\
             {% for item in __content__.items %}<i>{{ item }}</i>{% endfor %}",
        );
        site.write(
            "root/blog/post.json",
            "---\ntemplate: post.html\nvars:\n  title: Hello\n  views: 42\n---\n{\"items\": [\"a\", \"b\"]}",
        );

        site.build().unwrap();

        let page = site.read("blog/post/index.html");
        assert!(page.contains("<h1>Hello</h1>"));
        assert!(page.contains("<span>42</span>"));
        assert!(page.contains("<i>a</i><i>b</i>"));
    }

    #[test]
    fn copies_a_content_file_without_frontmatter() {
        let site = Site::new();
        site.write("root/blog/notes.md", "# Just content\n");
        site.write("root/blog/data.json", r#"{"items": ["a"]}"#);

        site.build().unwrap();

        assert_eq!(site.read("blog/notes.md"), "# Just content\n");
        assert_eq!(site.read("blog/data.json"), r#"{"items": ["a"]}"#);
        assert!(!site.exists("blog/notes.html"));
    }

    #[test]
    fn omits_draft_files_in_production_builds() {
        let site = Site::new();
        site.write("templates/post.html", "{{__content__}}");
        site.write(
            "root/blog/post.md",
            "---\ntemplate: post.html\nvars: {}\n---\n# published\n",
        );
        site.write(
            "root/blog/post.draft.md",
            "---\ntemplate: post.html\nvars: {}\n---\n# unpublished\n",
        );
        site.write("root/assets/notes.draft.txt", "wip\n");

        site.build().unwrap();

        assert!(site.read("blog/post/index.html").contains("published"));
        assert!(!site.exists("blog/post.draft/index.html"));
        assert!(!site.exists("blog/post.draft.md"));
        assert!(!site.exists("assets/notes.draft.txt"));
    }

    #[test]
    fn keeps_draft_files_in_development_builds() {
        let site = Site::new().with_mode(BuildMode::Dev);
        site.write("templates/post.html", "{{__content__}}");
        site.write(
            "root/blog/post.draft.md",
            "---\ntemplate: post.html\nvars: {}\n---\n# unpublished\n",
        );
        site.write("root/assets/notes.draft.txt", "wip\n");

        site.build().unwrap();

        assert!(
            site.read("blog/post.draft/index.html")
                .contains("unpublished")
        );
        assert_eq!(site.read("assets/notes.draft.txt"), "wip\n");
    }

    #[test]
    fn renders_flat_pages_when_clean_urls_are_off() {
        let site = Site::with_layout(OutputLayout::new(false, false));
        site.write("templates/post.html", "{{__content__}}");
        site.write(
            "root/blog/post.md",
            "---\ntemplate: post.html\nvars: {}\n---\n# body\n",
        );

        site.build().unwrap();

        assert!(site.read("blog/post.html").contains("<h1 id=\"body\">"));
        assert!(!site.exists("blog/post/index.html"));
    }

    #[test]
    fn keeps_index_and_404_pages_at_their_place_with_clean_urls() {
        let site = Site::new();
        site.write("templates/post.html", "{{__content__}}");
        site.write(
            "root/index.md",
            "---\ntemplate: post.html\nvars: {}\n---\n# home\n",
        );
        site.write(
            "root/blog/index.md",
            "---\ntemplate: post.html\nvars: {}\n---\n# blog\n",
        );
        site.write(
            "root/404.md",
            "---\ntemplate: post.html\nvars: {}\n---\n# missing\n",
        );

        site.build().unwrap();

        assert!(site.read("index.html").contains("<h1 id=\"home\">"));
        assert!(site.read("blog/index.html").contains("<h1 id=\"blog\">"));
        assert!(site.read("404.html").contains("<h1 id=\"missing\">"));
        assert!(!site.exists("index/index.html"));
        assert!(!site.exists("404/index.html"));
    }

    #[test]
    fn fingerprints_assets_and_resolves_them_in_templates() {
        let site = Site::with_layout(OutputLayout::new(true, true));
        site.write(
            "templates/post.html",
            r#"<link rel="stylesheet" href="{{ asset(path="assets/style.css") }}">{{__content__}}"#,
        );
        site.write(
            "root/blog/post.md",
            "---\ntemplate: post.html\nvars: {}\n---\nbody\n",
        );
        site.write("root/assets/style.css", "body {}\n");

        site.build().unwrap();

        let page = site.read("blog/post/index.html");
        let link = href(&page).to_string();
        assert!(
            link.starts_with("/assets/style."),
            "unexpected link: {link}"
        );
        // The link and the file the build wrote carry the same name
        let built = link.trim_start_matches('/');
        assert_eq!(site.read(built), "body {}\n");
    }

    #[test]
    fn keeps_asset_names_when_fingerprinting_is_off() {
        let site = Site::new();
        site.write(
            "templates/post.html",
            r#"<link href="{{ asset(path="assets/style.css") }}">"#,
        );
        site.write(
            "root/blog/post.md",
            "---\ntemplate: post.html\nvars: {}\n---\nbody\n",
        );
        site.write("root/assets/style.css", "body {}\n");

        site.build().unwrap();

        assert!(
            site.read("blog/post/index.html")
                .contains("/assets/style.css")
        );
        assert_eq!(site.read("assets/style.css"), "body {}\n");
    }

    #[test]
    fn fails_on_an_asset_which_does_not_exist() {
        let site = Site::new();
        site.write(
            "templates/post.html",
            r#"<link href="{{ asset(path="assets/typo.css") }}">"#,
        );
        site.write(
            "root/blog/post.md",
            "---\ntemplate: post.html\nvars: {}\n---\nbody\n",
        );

        let error = format!("{}", Report::from_error(site.build().unwrap_err()));

        assert!(error.contains("typo.css"), "unexpected error: {error}");
        assert!(
            error.contains("Could not render blog/post.md"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn fails_on_a_content_file_without_a_template() {
        let site = Site::new();
        site.write(
            "root/blog/post.md",
            "---\nvars:\n  title: Hello\n---\n# body\n",
        );

        let error = format!("{}", Report::from_error(site.build().unwrap_err()));

        assert!(error.contains("post.md"), "unexpected error: {error}");
        assert!(error.contains("template"), "unexpected error: {error}");
    }

    #[test]
    fn fails_on_invalid_json_content() {
        let site = Site::new();
        site.write("templates/post.html", "{{__content__}}");
        site.write(
            "root/blog/post.json",
            "---\ntemplate: post.html\nvars: {}\n---\n{",
        );

        let error = format!("{}", Report::from_error(site.build().unwrap_err()));

        assert!(
            error.contains("Could not parse JSON content"),
            "unexpected error: {error}"
        );
        assert!(error.contains("post.json"), "unexpected error: {error}");
    }

    #[test]
    fn renders_markdown_content_files() {
        let site = Site::new();
        site.write(
            "templates/post.html",
            "<h1>{{ vars.title }}</h1>{{__content__}}",
        );
        site.write(
            "root/blog/post.md",
            "---\ntemplate: post.html\nvars:\n  title: Hello\n---\n# Heading\n\nSome *emphasis* inside <b>HTML</b>.\n",
        );

        site.build().unwrap();

        let page = site.read("blog/post/index.html");
        assert!(page.contains("<h1>Hello</h1>"));
        assert!(page.contains("<h1 id=\"heading\">Heading</h1>"));
        assert!(page.contains("<em>emphasis</em>"));
        // Raw HTML written by the author passes through by default
        assert!(page.contains("<b>HTML</b>"));
    }

    #[test]
    fn omits_raw_html_when_build_disables_it() {
        let site = Site::new();
        site.write("templates/post.html", "{{__content__}}");
        site.write(
            "root/blog/post.md",
            "---\ntemplate: post.html\nvars:\n  title: Hello\n---\nSome <b>HTML</b>.\n",
        );

        site.build_with(MarkdownParserOptions::default().with_raw_html(false))
            .unwrap();

        assert!(
            site.read("blog/post/index.html")
                .contains("<!-- raw HTML omitted -->")
        );
    }

    #[test]
    fn keeps_the_previous_output_when_a_build_fails() {
        let site = Site::new();
        site.write("templates/post.html", "{{__content__}}");
        site.write(
            "root/blog/post.md",
            "---\ntemplate: post.html\nvars:\n  title: Hello\n---\n# body\n",
        );
        site.build().unwrap();

        site.write(
            "root/blog/post.md",
            "---\ntemplate: post.html\nvars:\n  title: [unclosed\n---\n# body\n",
        );

        assert!(site.build().is_err());
        assert!(
            site.read("blog/post/index.html")
                .contains("<h1 id=\"body\">")
        );

        let mut staging = site.roots.out().as_os_str().to_os_string();
        staging.push(".staging");
        assert!(!std::path::Path::new(&staging).exists());
    }

    #[test]
    fn lets_a_page_list_its_siblings_and_skip_itself() {
        let site = Site::new();
        site.write("templates/post.html", "<h1>{{ vars.title }}</h1>");
        site.write(
            "templates/list.html",
            "{% for page in pages(pattern=\"blog/**/*.md\") %}\
             {% if page.url != __url__ %}<a href=\"{{ page.url }}\">{{ page.vars.title }}</a>{% endif %}\
             {% endfor %}",
        );
        site.write("root/blog/index.md", "---\ntemplate: list.html\n---\n");
        site.write(
            "root/blog/first.md",
            "---\ntemplate: post.html\nvars:\n  title: First\n---\n",
        );
        site.write(
            "root/blog/nested/second.md",
            "---\ntemplate: post.html\nvars:\n  title: Second\n---\n",
        );
        // A file the build copies is not a page, so it never shows up in a listing
        site.write("root/blog/diagram.svg", "<svg/>");

        site.build().unwrap();

        let listing = site.read("blog/index.html");
        assert!(
            listing.contains("<a href=\"/blog/first\">First</a>"),
            "unexpected listing: {listing}"
        );
        assert!(
            listing.contains("<a href=\"/blog/nested/second\">Second</a>"),
            "unexpected listing: {listing}"
        );
        // Two posts are linked; `__url__` is the listing's own URL, which is how
        // it skips its own entry
        assert_eq!(listing.matches("<a href=").count(), 2, "listing: {listing}");
    }

    #[test]
    fn fails_on_an_invalid_pattern_given_to_the_pages_function() {
        let site = Site::new();
        site.write(
            "templates/list.html",
            "{% for page in pages(pattern=\"blog/[\") %}{{ page.url }}{% endfor %}",
        );
        site.write("root/blog/index.md", "---\ntemplate: list.html\n---\n");

        let error = format!("{}", Report::from_error(site.build().unwrap_err()));

        assert!(error.contains("pages()"), "unexpected error: {error}");
    }
}
