use crate::{
    parsers::{ContentParser, ParsedMetadata, RawHtml, StringMap},
    path::AbsPath,
    templates::{Template, Templater, VAR_SLOT},
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
pub struct ContentSource(AbsPath);

impl ContentSource {
    pub fn new(path: AbsPath) -> Self {
        Self(path)
    }

    pub fn inner(&self) -> &AbsPath {
        &self.0
    }

    pub fn into_inner(self) -> AbsPath {
        self.0
    }

    pub fn kind(&self) -> anyhow::Result<ContentKind> {
        let extension = self
            .0
            .inner()
            .extension()
            .and_then(|s| s.to_str())
            .ok_or_else(|| {
                anyhow::anyhow!("Could not extract extension from {:?}", self.inner())
            })?;
        let val = match extension {
            "md" => ContentKind::Markdown,
            "html" => ContentKind::Html,
            "json" => ContentKind::Json,
            _ => anyhow::bail!("Extension {:?} could not be parsed!", extension),
        };
        Ok(val)
    }

    pub fn parse<P: ContentParser>(self, parser: &P) -> anyhow::Result<ContentParsed> {
        let content = std::fs::read_to_string(self.0.inner())?;
        let (metadata, body) = parser.parse(&content)?;
        Ok(ContentParsed::new(self.0, metadata, body))
    }
}

#[derive(Debug, Clone)]
pub struct ContentParsed {
    original_file_path: AbsPath,
    metadata: ParsedMetadata,
    content: RawHtml,
}

impl ContentParsed {
    pub fn new(original_file_path: AbsPath, metadata: ParsedMetadata, content: RawHtml) -> Self {
        Self {
            original_file_path,
            metadata,
            content,
        }
    }

    pub fn path(&self) -> &AbsPath {
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
    pub fn into_map(self) -> anyhow::Result<StringMap> {
        let mut map = self.metadata.into_inner();
        if map.contains_key(VAR_SLOT) {
            anyhow::bail!("Frontmatter variable {VAR_SLOT:?} collides with the content slot");
        }
        map.insert(VAR_SLOT.to_string(), self.content.into_inner());
        Ok(map)
    }

    pub fn render<T: Templater>(
        self,
        templater: &T,
        template: &Template,
    ) -> anyhow::Result<RenderedContent> {
        let values = self.into_map()?;
        let rendered_content = templater.render(template.name(), &values)?;
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

    fn source(path: &str) -> ContentSource {
        ContentSource::new(AbsPath::try_from(PathBuf::from(path)).unwrap())
    }

    fn element(metadata: &[(&str, &str)]) -> ContentParsed {
        ContentParsed::new(
            AbsPath::try_from(PathBuf::from("/site/root/post.html")).unwrap(),
            ParsedMetadata::new(
                metadata
                    .iter()
                    .map(|(key, value)| (key.to_string(), value.to_string()))
                    .collect(),
            ),
            RawHtml::new("<p>body</p>".to_string()),
        )
    }

    #[test]
    fn maps_source_extensions_to_content_kinds() {
        assert!(matches!(
            source("/site/root/post.md").kind().unwrap(),
            ContentKind::Markdown
        ));
        assert!(matches!(
            source("/site/root/post.html").kind().unwrap(),
            ContentKind::Html
        ));
        assert!(source("/site/root/style.css").kind().is_err());
    }

    #[test]
    fn parses_a_source_file_into_path_metadata_and_body() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("post.html");
        std::fs::write(&file, "---\ntitle: hi\n---\n<p>body</p>").unwrap();
        let source = ContentSource::new(AbsPath::try_from(file).unwrap());

        let parsed = source.parse(&HtmlParser::new()).unwrap();

        assert_eq!(
            parsed.metadata().inner().get("title"),
            Some(&"hi".to_string())
        );
        assert_eq!(parsed.content().inner(), "<p>body</p>");
        assert!(parsed.path().inner().ends_with("post.html"));
    }

    #[test]
    fn adds_the_content_slot_to_the_variable_map() {
        let map = element(&[("title", "hi")]).into_map().unwrap();
        assert_eq!(map.get("title"), Some(&"hi".to_string()));
        assert_eq!(map.get(VAR_SLOT), Some(&"<p>body</p>".to_string()));
    }

    #[test]
    fn rejects_a_frontmatter_collision_with_the_slot() {
        assert!(element(&[(VAR_SLOT, "hijacked")]).into_map().is_err());
    }
}
