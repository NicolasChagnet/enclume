use crate::{
    output::Output,
    parsers::{ContentKind, ParsedData, Parsers, RawHtml},
    path::{Roots, SitePath},
    templates::Templater,
};
use snafu::prelude::*;
use std::{fmt, path::PathBuf};

/// Number of hex characters of a content hash kept in a fingerprinted name
const FINGERPRINT_LENGTH: usize = 12;

/// A stage of the content pipeline, tied to the file it came from
pub trait Content {
    /// The source file behind this stage
    fn file(&self) -> &ContentFile;

    /// Write the artifact into the output tree, creating the parent directories
    fn write(&self, output: &Output) -> Result<(), snafu::Whatever>;
}

/// A file below the content root
///
/// The site-relative path is the identity of the file: the output layout, the
/// `asset()` template function and every error message use it.
#[derive(Debug, Clone)]
pub struct ContentFile {
    source: PathBuf,
    path: SitePath,
    kind: ContentKind,
}

impl ContentFile {
    /// Locate a file below the content root
    pub fn locate(roots: &Roots, path: SitePath) -> Result<Self, snafu::Whatever> {
        let kind = ContentKind::from_path(&path);
        let source = path.resolve(&roots.content_dir());
        Ok(Self { source, path, kind })
    }

    /// The site-relative path of the file
    pub fn path(&self) -> &SitePath {
        &self.path
    }

    pub fn kind(&self) -> ContentKind {
        self.kind
    }

    /// The content of the file
    pub fn read(&self) -> Result<String, snafu::Whatever> {
        std::fs::read_to_string(&self.source).whatever_context(format!("Couldn't read {self}"))
    }

    /// The content hash used in fingerprinted file names
    pub fn fingerprint(&self) -> Result<String, snafu::Whatever> {
        let content =
            std::fs::read(&self.source).whatever_context(format!("Couldn't read {self}"))?;
        let hash = blake3::hash(&content).to_hex().to_string();
        Ok(hash[..FINGERPRINT_LENGTH].to_string())
    }

    /// Read and parse the file
    ///
    /// `None` marks a file which declares no metadata, and which the build copies
    /// verbatim, and a file which no parser handles.
    pub fn parse(&self, parsers: &Parsers) -> Result<Option<ParsedContent>, snafu::Whatever> {
        let Some(parser) = parsers.parser_for(self.kind) else {
            return Ok(None);
        };
        let content = self.read()?;
        let Some(metadata) = parser.parse(&content, &self.path)? else {
            return Ok(None);
        };
        Ok(Some(ParsedContent::new(self.clone(), metadata)))
    }
}

impl Content for ContentFile {
    fn file(&self) -> &ContentFile {
        self
    }

    /// Copy the file to the name the build recorded for it
    fn write(&self, output: &Output) -> Result<(), snafu::Whatever> {
        let destination = output.asset(&self.path);
        output
            .copy(&self.source, &destination)
            .with_whatever_context(|_| format!("Could not copy {self}"))
    }
}

impl fmt::Display for ContentFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.path.fmt(f)
    }
}

/// Metadata of a file, waiting for a render pass
#[derive(Debug, Clone)]
pub struct ParsedContent {
    file: ContentFile,
    metadata: ParsedData,
}

impl ParsedContent {
    pub fn new(file: ContentFile, metadata: ParsedData) -> Self {
        Self { file, metadata }
    }

    pub fn file(&self) -> &ContentFile {
        &self.file
    }

    /// The site-relative path of the file this content comes from
    pub fn path(&self) -> &SitePath {
        self.file.path()
    }

    pub fn metadata(&self) -> &ParsedData {
        &self.metadata
    }

    /// Render the content through the template it declares, for the page served at `url`
    pub fn render<T: Templater>(
        &self,
        templater: &T,
        url: &str,
    ) -> Result<RenderedContent, snafu::Whatever> {
        let html = templater.render(self.metadata.clone(), url)?;
        Ok(RenderedContent::new(self.file.clone(), html))
    }
}

/// HTML of a file, ready to be written
#[derive(Debug, Clone)]
pub struct RenderedContent {
    file: ContentFile,
    html: RawHtml,
}

impl RenderedContent {
    pub fn new(file: ContentFile, html: RawHtml) -> Self {
        Self { file, html }
    }

    pub fn inner(&self) -> &RawHtml {
        &self.html
    }

    pub fn as_str(&self) -> &str {
        self.html.inner()
    }
}

impl Content for RenderedContent {
    fn file(&self) -> &ContentFile {
        &self.file
    }

    /// Write the HTML at the page destination of the source file
    fn write(&self, output: &Output) -> Result<(), snafu::Whatever> {
        let destination = output.page(self.file.path())?;
        output
            .write(&destination, self.html.inner().as_bytes())
            .with_whatever_context(|_| format!("Could not write {}", self.file))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsers::MarkdownParserOptions;
    use std::path::Path;

    /// Content root holding the files a test writes
    struct Site {
        // Held to remove the temporary directory once the test ends
        _dir: tempfile::TempDir,
        roots: Roots,
    }

    impl Site {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            std::fs::create_dir(dir.path().join("src")).unwrap();
            let roots = Roots::new(dir.path().join("src"), dir.path().join("dist")).unwrap();
            Self { _dir: dir, roots }
        }

        /// Write a file below the content root
        fn write(&self, relative: &str, content: &str) -> ContentFile {
            let path = SitePath::try_from(PathBuf::from(relative)).unwrap();
            let file = ContentFile::locate(&self.roots, path).unwrap();
            std::fs::create_dir_all(file.source.parent().unwrap()).unwrap();
            std::fs::write(&file.source, content).unwrap();
            file
        }
    }

    fn parsers() -> Parsers {
        Parsers::new(MarkdownParserOptions::default())
    }

    #[test]
    fn maps_source_extensions_to_content_kinds() {
        let site = Site::new();

        for (relative, kind) in [
            ("blog/post.md", ContentKind::Markdown),
            ("blog/post.json", ContentKind::Json),
            ("blog/post.html", ContentKind::Html),
            ("blog/post.yaml", ContentKind::Yaml),
            ("assets/style.css", ContentKind::Other),
        ] {
            assert_eq!(site.write(relative, "").kind(), kind, "{relative}");
        }
    }

    #[test]
    fn parses_a_source_file_into_path_and_metadata() {
        let site = Site::new();
        let file = site.write(
            "blog/post.md",
            "---\ntemplate: post.html\nvars:\n  title: hi\n---\nbody",
        );

        let parsed = file.parse(&parsers()).unwrap().unwrap();

        let value = parsed.metadata().as_value().unwrap();
        assert_eq!(value["vars"]["title"], serde_json::json!("hi"));
        assert_eq!(value["__content__"], serde_json::json!("<p>body</p>\n"));
        assert_eq!(
            parsed.metadata().template().as_path(),
            Path::new("post.html")
        );
        assert_eq!(parsed.path().as_path(), Path::new("blog/post.md"));
    }

    #[test]
    fn skips_a_source_file_without_frontmatter() {
        let site = Site::new();
        let file = site.write("blog/post.md", "# Just content\n");

        assert!(file.parse(&parsers()).unwrap().is_none());
    }

    #[test]
    fn copies_a_file_which_no_parser_handles() {
        let site = Site::new();
        let file = site.write("assets/style.css", "body {}\n");

        assert_eq!(file.kind(), ContentKind::Other);
        assert!(file.parse(&parsers()).unwrap().is_none());
    }

    #[test]
    fn reports_a_parse_error() {
        let site = Site::new();
        let file = site.write(
            "blog/post.json",
            "---\ntemplate: post.html\nvars: {}\n---\n{",
        );

        let error = file.parse(&parsers()).unwrap_err();

        assert!(
            format!("{error}").contains("Could not parse JSON content"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn fingerprints_a_file_from_its_content() {
        let site = Site::new();
        let digest = site
            .write("assets/style.css", "body {}\n")
            .fingerprint()
            .unwrap();

        assert_eq!(digest.len(), FINGERPRINT_LENGTH);
        assert!(digest.chars().all(|c| c.is_ascii_hexdigit()));
        // The same content gives the same digest, different content does not
        assert_eq!(
            site.write("assets/other.css", "body {}\n")
                .fingerprint()
                .unwrap(),
            digest
        );
        assert_ne!(
            site.write("assets/third.css", "body { color: red }\n")
                .fingerprint()
                .unwrap(),
            digest
        );
    }
}
