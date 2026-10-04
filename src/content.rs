use std::path::Path;

use crate::{
    parsers::{ContentParser, ParsedMetadata, RawHtml, Var, VarMap},
    path::SitePath,
    templates::{Templater, VAR_SLOT},
};

#[derive(Debug, Clone, Copy)]
pub enum ContentKind {
    Markdown,
    Html,
    Json,
}

impl ContentKind {
    pub fn try_from_extension(extension: &str) -> anyhow::Result<Self> {
        let val = match extension {
            "md" => Self::Markdown,
            "html" => Self::Html,
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

    pub fn parse<P: ContentParser>(self, parser: &P) -> anyhow::Result<ContentParsed> {
        let content = std::fs::read_to_string(self.content_dir.join(self.file.as_path()))?;
        let (metadata, body) = parser.parse(&content)?;
        Ok(ContentParsed::new(self.file, metadata, body))
    }
}

#[derive(Debug, Clone)]
pub struct ContentParsed {
    original_file_path: SitePath,
    metadata: ParsedMetadata,
    content: RawHtml,
}

impl ContentParsed {
    pub fn new(original_file_path: SitePath, metadata: ParsedMetadata, content: RawHtml) -> Self {
        Self {
            original_file_path,
            metadata,
            content,
        }
    }

    pub fn path(&self) -> &SitePath {
        &self.original_file_path
    }

    pub fn metadata(&self) -> &ParsedMetadata {
        &self.metadata
    }

    pub fn content(&self) -> &RawHtml {
        &self.content
    }

    /// Flatten the element into the variable map handed to templates
    ///
    /// Errors when the frontmatter already declares the content slot variable,
    /// which would otherwise be overwritten without notice.
    pub fn into_map(self) -> anyhow::Result<VarMap> {
        let mut map = self.metadata.into_inner();
        if map.contains_key(VAR_SLOT) {
            anyhow::bail!("Frontmatter variable {VAR_SLOT:?} collides with the content slot");
        }
        map.insert(VAR_SLOT.to_string(), Var::String(self.content.into_inner()));
        Ok(map)
    }

    pub fn render<T: Templater>(
        self,
        templater: &T,
        template: &str,
    ) -> anyhow::Result<RenderedContent> {
        let values = self.into_map()?;
        let rendered_content = templater.render(template, values)?;
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
    use super::*;
    use crate::parsers::HtmlParser;
    use std::path::PathBuf;

    fn site_path(path: &str) -> SitePath {
        SitePath::try_from(PathBuf::from(path)).unwrap()
    }

    fn element(metadata: &[(&str, &str)]) -> ContentParsed {
        ContentParsed::new(
            site_path("blog/post.html"),
            ParsedMetadata::new(
                metadata
                    .iter()
                    .map(|(key, value)| (key.to_string(), serde_json::json!(value)))
                    .collect(),
            ),
            RawHtml::new("<p>body</p>".to_string()),
        )
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
    fn parses_a_source_file_into_path_metadata_and_body() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("post.html"),
            "---\ntitle: hi\n---\n<p>body</p>",
        )
        .unwrap();
        let source = ContentSource::new(dir.path(), site_path("post.html"));

        let parsed = source.parse(&HtmlParser::new()).unwrap();

        assert_eq!(
            parsed.metadata().inner().get("title"),
            Some(&serde_json::json!("hi"))
        );
        assert_eq!(parsed.content().inner(), "<p>body</p>");
        assert_eq!(parsed.path().as_path(), Path::new("post.html"));
    }

    #[test]
    fn adds_the_content_slot_to_the_variable_map() {
        let map = element(&[("title", "hi")]).into_map().unwrap();
        assert_eq!(map.get("title"), Some(&serde_json::json!("hi")));
        assert_eq!(map.get(VAR_SLOT), Some(&serde_json::json!("<p>body</p>")));
    }

    #[test]
    fn rejects_a_frontmatter_collision_with_the_slot() {
        assert!(element(&[(VAR_SLOT, "hijacked")]).into_map().is_err());
    }
}
