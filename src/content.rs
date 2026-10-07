use std::path::Path;

use crate::{
    parsers::{ContentParser, ParsedMetadata, RawHtml},
    path::SitePath,
    templates::Templater,
};

#[derive(Debug, Clone, Copy)]
pub enum ContentKind {
    Markdown,
    Json,
}

impl ContentKind {
    pub fn try_from_extension(extension: &str) -> anyhow::Result<Self> {
        let val = match extension {
            "md" => Self::Markdown,
            "json" => Self::Json,
            _ => anyhow::bail!("Extension {:?} could not be parsed!", extension),
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

    pub fn kind(&self) -> anyhow::Result<ContentKind> {
        let extension = self
            .file
            .as_path()
            .extension()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow::anyhow!("Could not extract extension from {}", self.file))?;
        ContentKind::try_from_extension(extension)
    }

    /// Parse the file into an element
    ///
    /// `None` marks a file which declares no metadata, and which the build
    /// copies verbatim instead of rendering.
    pub fn parse<P: ContentParser>(self, parser: &P) -> anyhow::Result<Option<ContentParsed>> {
        let content = std::fs::read_to_string(self.content_dir.join(self.file.as_path()))?;
        let Some(metadata) = parser.parse(&content)? else {
            return Ok(None);
        };
        Ok(Some(ContentParsed::new(self.file, metadata)))
    }
}

#[derive(Debug, Clone)]
pub struct ContentParsed {
    original_file_path: SitePath,
    metadata: ParsedMetadata,
}

impl ContentParsed {
    pub fn new(original_file_path: SitePath, metadata: ParsedMetadata) -> Self {
        Self {
            original_file_path,
            metadata,
        }
    }

    pub fn path(&self) -> &SitePath {
        &self.original_file_path
    }

    pub fn metadata(&self) -> &ParsedMetadata {
        &self.metadata
    }

    /// Flatten the element into the variable map handed to templates
    pub fn render<T: Templater>(self, templater: &T) -> anyhow::Result<RenderedContent> {
        let (template, values) = self.metadata.into_attrs();
        let rendered_content = templater.render(&template, values)?;
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
    use crate::parsers::{MarkdownParser, VAR_CONTENT};

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
            "---\ntitle: hi\ntemplate: post.html\n---\nbody",
        )
        .unwrap();
        let source = ContentSource::new(dir.path(), site_path("post.md"));

        let parsed = source.parse(&MarkdownParser::default()).unwrap().unwrap();

        assert_eq!(
            parsed.metadata().variables().get("title"),
            Some(&serde_json::json!("hi"))
        );
        assert_eq!(
            parsed.metadata().variables().get(VAR_CONTENT),
            Some(&serde_json::json!("<p>body</p>\n"))
        );
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
