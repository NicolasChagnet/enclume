use std::path::{Path, PathBuf};

use rayon::prelude::*;
use snafu::prelude::*;

use crate::{
    content::{ContentKind, ContentSource},
    glob::glob_files,
    parsers::{HtmlParser, JsonParser, MarkdownParser, MarkdownParserOptions},
    path::{Roots, SitePath},
    templates::TeraTemplater,
};

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
    markdown_options: MarkdownParserOptions,
}

impl SiteBuilder {
    pub fn new(roots: Roots, markdown_options: MarkdownParserOptions) -> Self {
        Self {
            roots,
            markdown_options,
        }
    }

    /// Build every file into `out`
    fn build_into(&self, out: &Path) -> Result<(), snafu::Whatever> {
        reset_dir(out)?;

        let content_dir = self.roots.content_dir();
        let content_files = get_all_files(&content_dir)?;

        let mut templater = TeraTemplater::new();
        templater.load_templates(&self.roots.templates_dir())?;
        let markdown_parser = MarkdownParser::new(self.markdown_options);

        // Loop over every file inside the input directory
        content_files.into_par_iter().try_for_each(|file| {
            log::debug!("Handling {file}");

            let destination = out.join(file.as_path());
            let source = ContentSource::new(&content_dir, file.clone());
            let parsed = match source.kind() {
                // Content files name their rendering template in their metadata
                Ok(kind) => match kind {
                    ContentKind::Markdown => source.parse(&markdown_parser),
                    ContentKind::Json => source.parse(&JsonParser),
                    ContentKind::Html => source.parse(&HtmlParser),
                }
                .with_whatever_context(|_| format!("Could not parse {file}"))?,
                // Files without a known content kind have no metadata either
                Err(_) => None,
            };

            // Files which declare no metadata are copied verbatim
            let Some(parsed) = parsed else {
                return copy_file_destination(&content_dir.join(file.as_path()), &destination)
                    .with_whatever_context(|_| {
                        format!("Could not copy {file} into {}", out.display())
                    });
            };

            let content = parsed
                .render(&templater)
                .with_whatever_context(|_| format!("Could not render {file}"))?;
            write_to_file(&destination.with_extension("html"), content.as_str())
                .with_whatever_context(|_| format!("Could not write {file}"))?;
            Ok(())
        })?;
        Ok(())
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

/// Write rendered content to a file
fn write_to_file(path: &Path, content: &str) -> Result<(), snafu::Whatever> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .whatever_context(format!("Couldn't create parent directory {parent:?}"))?;
    }
    std::fs::write(path, content).whatever_context(format!("Couldn't write file {path:?}"))?;
    Ok(())
}

/// Copy a file to the destination folder as-is
fn copy_file_destination(
    origin_path: &Path,
    destination_path: &Path,
) -> Result<(), snafu::Whatever> {
    if let Some(parent) = destination_path.parent() {
        std::fs::create_dir_all(parent)
            .whatever_context(format!("Couldn't create parent directory {parent:?}"))?;
    }
    std::fs::copy(origin_path, destination_path).whatever_context(format!(
        "Couldn't copy {origin_path:?} to {destination_path:?}"
    ))?;
    Ok(())
}

/// Get all the files within the content root, as site-relative paths, sorted for a deterministic build
fn get_all_files(content_dir: &Path) -> Result<Vec<SitePath>, snafu::Whatever> {
    let pattern = content_dir.join("**/*");
    let mut files = glob_files(&pattern)?
        .into_iter()
        .map(|path| {
            let relative = path.strip_prefix(content_dir).whatever_context(format!(
                "File {} is outside {}",
                path.display(),
                content_dir.display()
            ))?;
            SitePath::try_from(relative.to_path_buf())
        })
        .collect::<Result<Vec<SitePath>, snafu::Whatever>>()?;
    files.sort();
    files.dedup();
    Ok(files)
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
    }

    impl Site {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            fs::create_dir(dir.path().join("src")).unwrap();
            let roots = Roots::new(dir.path().join("src"), dir.path().join("dist")).unwrap();
            Self { roots, _dir: dir }
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
            SiteBuilder::new(self.roots.clone(), markdown_options).build()
        }
    }

    #[test]
    fn renders_content_files_and_copies_other_files() {
        let site = Site::new();
        site.write(
            "templates/post.html",
            "<h1>{{ title | default(value=\"\") }}</h1>{{__content__}}",
        );
        site.write(
            "root/blog/post.md",
            "---\ntitle: Hello <b>world</b>\ntemplate: post.html\n---\nbody\n",
        );
        site.write(
            "root/blog/about.html",
            "---\ntitle: About <i>us</i>\ntemplate: post.html\n---\n<p>raw *html*</p>\n",
        );
        site.write("root/blog/other.html", "<p>no frontmatter</p>\n");
        site.write("root/assets/style.css", "body {}\n");

        site.build().unwrap();

        let page = site.read("blog/post.html");
        // Metadata is escaped, the parsed content is inserted as raw HTML
        assert!(page.contains("<h1>Hello &lt;b&gt;world&lt;/b&gt;</h1>"));
        assert!(page.contains("<p>body</p>"));
        let page = site.read("blog/about.html");
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
            "---\ntitle: Hello\ntemplate: post.html\n---\n# body\n",
        );

        site.build().unwrap();

        let page = site.read("blog/post.html");
        assert!(page.contains("<main>"));
        assert!(page.contains("<h1 id=\"body\">"));
    }

    #[test]
    fn renders_json_content_files() {
        let site = Site::new();
        site.write(
            "templates/post.html",
            "<h1>{{ title }}</h1><span>{{ views }}</span>\
             {% for item in __content__.items %}<i>{{ item }}</i>{% endfor %}",
        );
        site.write(
            "root/blog/post.json",
            "---\ntemplate: post.html\ntitle: Hello\nviews: 42\n---\n{\"items\": [\"a\", \"b\"]}",
        );

        site.build().unwrap();

        let page = site.read("blog/post.html");
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
    fn fails_on_a_content_file_without_a_template() {
        let site = Site::new();
        site.write("root/blog/post.md", "---\ntitle: Hello\n---\n# body\n");

        let error = format!("{}", Report::from_error(site.build().unwrap_err()));

        assert!(error.contains("post.md"), "unexpected error: {error}");
        assert!(error.contains("template"), "unexpected error: {error}");
    }

    #[test]
    fn fails_on_invalid_json_content() {
        let site = Site::new();
        site.write("templates/post.html", "{{__content__}}");
        site.write("root/blog/post.json", "---\ntemplate: post.html\n---\n{");

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
        site.write("templates/post.html", "<h1>{{ title }}</h1>{{__content__}}");
        site.write(
            "root/blog/post.md",
            "---\ntitle: Hello\ntemplate: post.html\n---\n# Heading\n\nSome *emphasis* inside <b>HTML</b>.\n",
        );

        site.build().unwrap();

        let page = site.read("blog/post.html");
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
            "---\ntitle: Hello\ntemplate: post.html\n---\nSome <b>HTML</b>.\n",
        );

        site.build_with(MarkdownParserOptions::default().with_raw_html(false))
            .unwrap();

        assert!(
            site.read("blog/post.html")
                .contains("<!-- raw HTML omitted -->")
        );
    }

    #[test]
    fn keeps_the_previous_output_when_a_build_fails() {
        let site = Site::new();
        site.write("templates/post.html", "{{__content__}}");
        site.write(
            "root/blog/post.md",
            "---\ntitle: Hello\ntemplate: post.html\n---\n# body\n",
        );
        site.build().unwrap();

        site.write(
            "root/blog/post.md",
            "---\ntitle: [unclosed\ntemplate: post.html\n---\n# body\n",
        );

        assert!(site.build().is_err());
        assert!(site.read("blog/post.html").contains("<h1 id=\"body\">"));

        let mut staging = site.roots.out().as_os_str().to_os_string();
        staging.push(".staging");
        assert!(!std::path::Path::new(&staging).exists());
    }
}
