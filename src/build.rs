use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::Context;
use log::info;
use rayon::prelude::*;

use crate::{
    collection::Collection,
    content::{ContentKind, ContentSource},
    glob::glob_files,
    parsers::HtmlParser,
    path::{AbsPath, CONTENT_DIR},
    templates::TeraTemplater,
};

/// Build the site from `base` into `out`
///
/// Wipes `out` first, renders every collection file through its template, then
/// copies the remaining content files as-is. Aborts on the first failure.
pub fn build(base: AbsPath, out: AbsPath) -> anyhow::Result<()> {
    ensure_disjoint(&base, &out)?;
    reset_dir(&out)?;

    let content_dir: AbsPath = base.inner().join(CONTENT_DIR).try_into()?;
    let content_files = get_all_files(&base)?;

    // Handle all collections
    let collections = Collection::load_all(&base, &content_files)?;
    let mut templater = TeraTemplater::new();
    templater.load_templates(collections.iter().map(|c| c.template()))?;
    let html_parser = HtmlParser::new();

    // Keep track of all files seen so the leftovers can be copied verbatim
    let mut remaining: HashSet<AbsPath> = content_files.iter().cloned().collect();

    for collection in &collections {
        info!("Parsing collection {}", collection);
        for file in collection.files() {
            remaining.remove(file);
        }

        collection.files().par_iter().try_for_each(|file| {
            let source_element = ContentSource::new(file.clone());
            let parser = match source_element.kind()? {
                ContentKind::Html => &html_parser,
                kind => anyhow::bail!("Parsing {kind:?} files is not implemented yet ({file})"),
            };
            let parsed_element = source_element.parse(parser)?;

            let element_path = parsed_element.path().to_owned();
            let destination_path = element_path.try_swap_base(content_dir.inner(), out.inner())?;

            let rendered_content = parsed_element
                .render(&templater, collection.template())
                .with_context(|| format!("Could not render {element_path}"))?;

            write_to_file(&destination_path, rendered_content.as_str())
                .with_context(|| format!("Could not write {destination_path}"))
        })?;
    }

    // Then copy every file which remains in the source
    remaining.par_iter().try_for_each(|file| {
        copy_file_destination(file, &content_dir, &out)
            .with_context(|| format!("Could not copy {file} from {content_dir} to {out}"))
    })
}

/// Refuse overlapping source and output trees before anything is deleted
fn ensure_disjoint(base: &AbsPath, out: &AbsPath) -> anyhow::Result<()> {
    if base.inner().starts_with(out.inner()) || out.inner().starts_with(base.inner()) {
        anyhow::bail!("Refusing to build: output directory {out} overlaps base directory {base}");
    }
    Ok(())
}

/// Delete `dir` and recreate it empty
///
/// Refuses directories which hold user data, such as the working directory
fn reset_dir(dir: &AbsPath) -> anyhow::Result<()> {
    ensure_deletable(dir)?;
    match std::fs::remove_dir_all(dir.inner()) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    std::fs::create_dir_all(dir.inner())?;
    Ok(())
}

/// Refuse deletion of directories which hold user data
fn ensure_deletable(dir: &AbsPath) -> anyhow::Result<()> {
    let path = dir.inner();
    let cwd = std::env::current_dir()?;
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if path.parent().is_none() || path == cwd.as_path() || home.as_deref() == Some(path) {
        anyhow::bail!("Refusing to delete {dir}: it is a root, home, or working directory");
    }
    Ok(())
}

/// Write rendered content to a file
fn write_to_file(path: &AbsPath, content: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.inner().parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path.inner(), content)?;
    Ok(())
}

/// Copy a file to the destination folder as-is
fn copy_file_destination(
    file: &AbsPath,
    content_dir: &AbsPath,
    out_dir: &AbsPath,
) -> anyhow::Result<()> {
    let destination_path = file.try_swap_base(content_dir.inner(), out_dir.inner())?;
    if let Some(parent) = destination_path.inner().parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(file.inner(), destination_path.inner())?;
    Ok(())
}

/// Get all the files within the root, sorted for a deterministic build
fn get_all_files(base_dir: &AbsPath) -> anyhow::Result<Vec<AbsPath>> {
    let pattern = base_dir.inner().join(CONTENT_DIR).join("**/*");
    let mut files = glob_files(&pattern)?;
    files.sort();
    files.dedup();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const COLLECTION: &str =
        "pattern = \"blog/**/*.{md,html}\"\ntemplate = \"post\"\nvariables = [\"title\"]\n";

    struct Site {
        // Held to remove the temporary directory once the test ends
        _dir: tempfile::TempDir,
        base: AbsPath,
        out: AbsPath,
    }

    impl Site {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            Self {
                base: AbsPath::try_from(dir.path().join("src")).unwrap(),
                out: AbsPath::try_from(dir.path().join("dist")).unwrap(),
                _dir: dir,
            }
        }

        fn write(&self, relative: &str, content: &str) {
            let path = self.base.inner().join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }

        fn read(&self, relative: &str) -> String {
            fs::read_to_string(self.out.inner().join(relative)).unwrap()
        }

        fn exists(&self, relative: &str) -> bool {
            self.out.inner().join(relative).exists()
        }

        fn build(&self) -> anyhow::Result<()> {
            build(self.base.clone(), self.out.clone())
        }
    }

    #[test]
    fn renders_collection_files_and_copies_other_content() {
        let site = Site::new();
        site.write(
            "templates/post.html",
            "<h1>{{ title | default(value=\"\") }}</h1>{{ slot }}",
        );
        site.write("collections/blog.toml", COLLECTION);
        site.write(
            "root/blog/post.html",
            "---\ntitle: Hello <b>world</b>\n---\n<p>body</p>\n",
        );
        site.write("root/blog/other.html", "<p>no frontmatter</p>\n");
        site.write("root/assets/style.css", "body {}\n");

        site.build().unwrap();

        let page = site.read("blog/post.html");
        // Metadata is escaped, the parsed content is inserted as raw HTML
        assert!(page.contains("<h1>Hello &lt;b&gt;world&lt;/b&gt;</h1>"));
        assert!(page.contains("<p>body</p>"));
        assert!(
            site.read("blog/other.html")
                .contains("<p>no frontmatter</p>")
        );
        assert_eq!(site.read("assets/style.css"), "body {}\n");
        assert!(!site.exists("root"));
    }

    #[test]
    fn fails_on_invalid_collection_file() {
        let site = Site::new();
        site.write(
            "collections/blog.toml",
            "pattern = \"blog/**/*.html\"\ntemplate = \"post\"\n",
        );

        let error = format!("{:#}", site.build().unwrap_err());

        assert!(error.contains("blog.toml"));
        assert!(error.contains("variables"));
    }

    #[test]
    fn fails_on_unsupported_content_kind() {
        let site = Site::new();
        site.write("templates/post.html", "{{ slot }}");
        site.write("collections/blog.toml", COLLECTION);
        site.write("root/blog/post.md", "---\ntitle: Hello\n---\n# body\n");

        let error = site.build().unwrap_err().to_string();

        assert!(error.contains("not implemented"));
        assert!(error.contains("post.md"));
    }

    #[test]
    fn fails_when_the_output_overlaps_the_base() {
        let site = Site::new();
        let inside_base = AbsPath::try_from(site.base.inner().join("dist")).unwrap();

        for out in [site.base.clone(), inside_base] {
            let error = build(site.base.clone(), out).unwrap_err().to_string();
            assert!(error.contains("overlaps"), "unexpected error: {error}");
        }
    }
}
