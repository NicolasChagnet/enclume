use crate::{
    parsers::{ContentParser, ParsedData, RawHtml},
    path::SitePath,
    templates::Templater,
};
use snafu::{FromString, prelude::*};
use std::path::Path;

/// Parseable content kinds
#[derive(Debug, Clone, Copy)]
pub enum ContentKind {
    Markdown,
    Json,
    Html,
}

impl ContentKind {
    pub fn try_from_extension(extension: &str) -> Result<Self, snafu::Whatever> {
        let val = match extension {
            "md" => Self::Markdown,
            "json" => Self::Json,
            "html" => Self::Html,
            _ => whatever!("Extension {:?} could not be parsed!", extension),
        };
        Ok(val)
    }
}

#[derive(Debug, Clone)]
pub struct ContentSource<'a> {
    content_dir: &'a Path,
    file: SitePath,
}

impl<'a> ContentSource<'a> {
    pub fn new(content_dir: &'a Path, file: SitePath) -> Self {
        Self { content_dir, file }
    }

    pub fn kind(&self) -> Result<ContentKind, snafu::Whatever> {
        let extension = self
            .file
            .as_path()
            .extension()
            .and_then(|s| s.to_str())
            .ok_or_else(|| {
                snafu::Whatever::without_source(format!(
                    "Could not extract extension from {}",
                    self.file
                ))
            })?;
        ContentKind::try_from_extension(extension)
    }

    /// Parse the file into an element
    ///
    /// `None` marks a file which declares no metadata, and which the build
    /// copies verbatim instead of rendering.
    pub fn parse<P: ContentParser>(
        self,
        parser: &P,
    ) -> Result<Option<ContentParsed>, snafu::Whatever> {
        let path = self.content_dir.join(self.file.as_path());
        let content = std::fs::read_to_string(&path)
            .whatever_context(format!("Couldn't read content of {path:?}"))?;
        let Some(metadata) = parser.parse(&content)? else {
            return Ok(None);
        };
        Ok(Some(ContentParsed::new(self.file, metadata)))
    }
}

#[derive(Debug, Clone)]
pub struct ContentParsed {
    original_file_path: SitePath,
    metadata: ParsedData,
}

impl ContentParsed {
    pub fn new(original_file_path: SitePath, metadata: ParsedData) -> Self {
        Self {
            original_file_path,
            metadata,
        }
    }

    pub fn path(&self) -> &SitePath {
        &self.original_file_path
    }

    pub fn metadata(&self) -> &ParsedData {
        &self.metadata
    }

    /// Render the content through the template it declares
    pub fn render<T: Templater>(self, templater: &T) -> Result<RenderedContent, snafu::Whatever> {
        let rendered_content = templater.render(self.metadata)?;
        Ok(RenderedContent::new(rendered_content))
    }
}

#[derive(Debug, Clone)]
pub struct RenderedContent(RawHtml);

impl RenderedContent {
    pub fn new(html: RawHtml) -> Self {
        Self(html)
    }

    pub fn inner(&self) -> &RawHtml {
        &self.0
    }

    pub fn as_str(&self) -> &str {
        self.0.inner()
    }
}

#[cfg(test)]
mod tests {
    use crate::parsers::MarkdownParser;

    use super::*;
    use std::path::PathBuf;

    fn site_path(path: &str) -> SitePath {
        SitePath::try_from(PathBuf::from(path)).unwrap()
    }

    #[test]
    fn maps_source_extensions_to_content_kinds() {
        let content_dir = Path::new("/site/root");

        assert!(matches!(
            ContentSource::new(content_dir, site_path("blog/post.md"))
                .kind()
                .unwrap(),
            ContentKind::Markdown
        ));
        assert!(matches!(
            ContentSource::new(content_dir, site_path("blog/post.json"))
                .kind()
                .unwrap(),
            ContentKind::Json
        ));
        assert!(matches!(
            ContentSource::new(content_dir, site_path("blog/post.html"))
                .kind()
                .unwrap(),
            ContentKind::Html
        ));
        assert!(
            ContentSource::new(content_dir, site_path("assets/style.css"))
                .kind()
                .is_err()
        );
    }

    #[test]
    fn parses_a_source_file_into_path_and_metadata() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("post.md"),
            "---\ntemplate: post.html\nvars:\n  title: hi\n---\nbody",
        )
        .unwrap();
        let source = ContentSource::new(dir.path(), site_path("post.md"));

        let parsed = source.parse(&MarkdownParser::default()).unwrap().unwrap();

        let value = parsed.metadata().as_value().unwrap();
        assert_eq!(value["vars"]["title"], serde_json::json!("hi"));
        assert_eq!(value["__content__"], serde_json::json!("<p>body</p>\n"));
        assert_eq!(
            parsed.metadata().template().as_path(),
            Path::new("post.html")
        );
        assert_eq!(parsed.path().as_path(), Path::new("post.md"));
    }

    #[test]
    fn skips_a_source_file_without_frontmatter() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("post.md"), "# Just content\n").unwrap();
        let source = ContentSource::new(dir.path(), site_path("post.md"));

        let parsed = source.parse(&MarkdownParser::default()).unwrap();

        assert!(parsed.is_none());
    }
}
