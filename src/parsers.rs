use anyhow::Context;
use rushdown::{parser::ParserExtension, renderer::html::RendererExtension};
use serde::Deserialize;

pub type Var = serde_json::Value;
pub type VarMap = std::collections::HashMap<String, Var>;

#[derive(Debug, Clone)]
pub struct RawHtml(String);

impl RawHtml {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    pub fn empty() -> Self {
        Self("".to_string())
    }

    pub fn inner(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

#[derive(Debug, Clone)]
pub struct ParsedMetadata(VarMap);

impl ParsedMetadata {
    pub fn new(value: VarMap) -> Self {
        Self(value)
    }

    pub fn inner(&self) -> &VarMap {
        &self.0
    }

    pub fn into_inner(self) -> VarMap {
        self.0
    }
}

pub trait ContentParser {
    /// Parse a string into metadata and a body
    fn parse(&self, content: &str) -> anyhow::Result<(ParsedMetadata, RawHtml)>;
}

#[derive(Debug, Clone)]
pub struct HtmlParser;

impl HtmlParser {
    pub fn new() -> Self {
        Self
    }
}

impl ContentParser for HtmlParser {
    fn parse(&self, content: &str) -> anyhow::Result<(ParsedMetadata, RawHtml)> {
        let (frontmatter, body) = split_yaml_frontmatter(content);
        let var_map = parse_yaml_frontmatter(frontmatter)?;

        Ok((ParsedMetadata::new(var_map), RawHtml::new(body.to_string())))
    }
}

/// Feature switches for the Markdown parser and HTML renderer
///
/// Every syntax extension is enabled by default, and raw HTML written in
/// Markdown passes through unless it is explicitly disabled.
#[derive(Clone, Copy, Debug)]
pub struct MarkdownParserOptions {
    auto_heading_ids: bool,
    allow_raw_html: bool,
    enable_footnotes: bool,
    enable_emojis: bool,
    enable_diagrams: bool,
    enable_fenced_divs: bool,
}

impl Default for MarkdownParserOptions {
    fn default() -> Self {
        Self {
            auto_heading_ids: true,
            allow_raw_html: true,
            enable_footnotes: true,
            enable_emojis: true,
            enable_diagrams: true,
            enable_fenced_divs: true,
        }
    }
}

impl MarkdownParserOptions {
    pub fn with_auto_heading_ids(mut self, enabled: bool) -> Self {
        self.auto_heading_ids = enabled;
        self
    }

    /// Pass author-written raw HTML through to the output
    ///
    /// Disabling it replaces raw HTML with an omission comment and drops
    /// unsafe links. Useful when the Markdown source is not fully trusted;
    /// the default suits a blog where the author writes the content.
    pub fn with_raw_html(mut self, enabled: bool) -> Self {
        self.allow_raw_html = enabled;
        self
    }

    pub fn with_footnotes(mut self, enabled: bool) -> Self {
        self.enable_footnotes = enabled;
        self
    }

    pub fn with_emojis(mut self, enabled: bool) -> Self {
        self.enable_emojis = enabled;
        self
    }

    pub fn with_diagrams(mut self, enabled: bool) -> Self {
        self.enable_diagrams = enabled;
        self
    }

    pub fn with_fenced_divs(mut self, enabled: bool) -> Self {
        self.enable_fenced_divs = enabled;
        self
    }

    /// Translate these options into rushdown parser options
    pub fn parser_options(&self) -> rushdown::parser::Options {
        rushdown::parser::Options {
            auto_heading_ids: self.auto_heading_ids,
            ..Default::default()
        }
    }

    /// Translate these options into rushdown HTML renderer options
    pub fn renderer_options(&self) -> rushdown::renderer::html::Options {
        rushdown::renderer::html::Options {
            allows_unsafe: self.allow_raw_html,
            ..Default::default()
        }
    }

    /// Assemble the parser extension holding every enabled syntax extension
    pub fn parser_extensions(&self) -> impl rushdown::parser::ParserExtension {
        let options = *self;

        rushdown::parser::empty_parser_extension().and(rushdown::parser::parser_extension(
            move |parser| {
                rushdown::parser::gfm(rushdown::parser::GfmOptions::default()).apply(parser);

                if options.enable_footnotes {
                    rushdown_footnote::footnote_parser_extension().apply(parser);
                }

                if options.enable_emojis {
                    rushdown_emoji::emoji_parser_extension(
                        rushdown_emoji::EmojiParserOptions::default(),
                    )
                    .apply(parser);
                }

                if options.enable_diagrams {
                    rushdown_diagram::diagram_parser_extension(
                        rushdown_diagram::DiagramParserOptions::default(),
                    )
                    .apply(parser);
                }

                if options.enable_fenced_divs {
                    rushdown_fenced_div::fenced_div_parser_extension().apply(parser);
                }
            },
        ))
    }

    /// Assemble the renderer extension holding every enabled syntax extension
    pub fn renderer_extensions(&self) -> impl rushdown::renderer::html::RendererExtension<'_> {
        let options = *self;

        rushdown::renderer::html::empty_renderer_extension().and(
            rushdown::renderer::html::renderer_extension(move |renderer| {
                if options.enable_footnotes {
                    rushdown_footnote::footnote_html_renderer_extension(
                        rushdown_footnote::FootnoteHtmlRendererOptions::default(),
                    )
                    .apply(renderer);
                }

                if options.enable_emojis {
                    rushdown_emoji::emoji_html_renderer_extension(
                        rushdown_emoji::EmojiHtmlRendererOptions::default(),
                    )
                    .apply(renderer);
                }

                if options.enable_diagrams {
                    rushdown_diagram::diagram_html_renderer_extension(
                        rushdown_diagram::DiagramHtmlRendererOptions::default(),
                    )
                    .apply(renderer);
                }

                if options.enable_fenced_divs {
                    rushdown_fenced_div::fenced_div_html_renderer_extension(
                        rushdown_fenced_div::FencedDivHtmlRendererOptions,
                    )
                    .apply(renderer);
                }
            }),
        )
    }
}

/// Markdown content parser backed by rushdown
#[derive(Clone, Debug)]
pub struct MarkdownParser {
    options: MarkdownParserOptions,
}

impl MarkdownParser {
    pub fn new(options: MarkdownParserOptions) -> Self {
        Self { options }
    }
}

impl ContentParser for MarkdownParser {
    fn parse(&self, content: &str) -> anyhow::Result<(ParsedMetadata, RawHtml)> {
        let (frontmatter, body) = split_yaml_frontmatter(content);
        let var_map = parse_yaml_frontmatter(frontmatter)?;

        let parser = rushdown::new_markdown_to_html(
            self.options.parser_options(),
            self.options.renderer_options(),
            self.options.parser_extensions(),
            self.options.renderer_extensions(),
        );

        let mut parsed_body = String::new();
        if let Err(e) = parser(&mut parsed_body, body) {
            // rushdown::Error is not Send, so anyhow cannot keep it as a source
            anyhow::bail!("Could not parse Markdown content: {e}")
        }
        Ok((ParsedMetadata::new(var_map), RawHtml::new(parsed_body)))
    }
}

/// Splits a string into a yaml frontmatter part and the rest
fn split_yaml_frontmatter(content: &str) -> (&str, &str) {
    // (?s) makes `.` match newlines. Group 1 is the yaml frontmatter body,
    // group 2 everything after the closing delimiter. The delimiters are left
    // out of group 1 because yaml_serde rejects documents with `---` markers.
    // Files without frontmatter keep their full content as body.
    let frontmatter_regex = regex::regex!(r"(?s)\A---\r?\n(.*?)\r?\n---\r?\n?(.*)\z");
    match frontmatter_regex.captures(content) {
        Some(caps) => (
            caps.get(1).map_or("", |m| m.as_str()),
            caps.get(2).map_or("", |m| m.as_str()),
        ),
        None => ("", content),
    }
}

/// Parse yaml frontmatter
fn parse_yaml_frontmatter(frontmatter: &str) -> anyhow::Result<VarMap> {
    let map = yaml_serde::from_str(frontmatter)?;
    Ok(map)
}

/// Shape of a JSON content file
///
/// The `metadata` field feeds the template variables, while `content` holds
/// arbitrary JSON exposed to templates as the [`VAR_CONTENT`] variable. JSON
/// files have no body, so the content slot stays empty.
#[derive(Debug, Clone, Deserialize)]
struct JsonContentFormat {
    metadata: Option<VarMap>,
    content: Option<Var>,
}

/// Variable holding the `content` field of a JSON content file
pub const VAR_CONTENT: &str = "content";

/// JSON content parser
///
/// Reserved variables: [`VAR_CONTENT`] holds the top-level `content` field,
/// and `slot` is injected empty by the build pipeline because JSON files have
/// no body. Metadata keys that shadow a reserved variable are overwritten
/// without notice.
pub struct JsonParser;

impl ContentParser for JsonParser {
    fn parse(&self, content: &str) -> anyhow::Result<(ParsedMetadata, RawHtml)> {
        let parsed_json: JsonContentFormat =
            serde_json::from_str(content).context("Could not parse JSON content")?;
        let mut metadata = parsed_json.metadata.unwrap_or_default();
        if let Some(content) = parsed_json.content {
            metadata.insert(VAR_CONTENT.to_string(), content);
        }
        Ok((ParsedMetadata::new(metadata), RawHtml::empty()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_frontmatter_from_body() {
        let (frontmatter, body) = split_yaml_frontmatter("---\ntitle: hi\n---\n<p>body</p>\n");
        assert_eq!(frontmatter, "title: hi");
        assert_eq!(body, "<p>body</p>\n");
    }

    #[test]
    fn keeps_content_without_frontmatter() {
        let (frontmatter, body) = split_yaml_frontmatter("<p>body</p>\n");
        assert_eq!(frontmatter, "");
        assert_eq!(body, "<p>body</p>\n");
    }

    #[test]
    fn handles_crlf_frontmatter() {
        let (frontmatter, body) =
            split_yaml_frontmatter("---\r\ntitle: hi\r\n---\r\n<p>body</p>\r\n");
        assert_eq!(frontmatter, "title: hi");
        assert_eq!(body, "<p>body</p>\r\n");
    }

    #[test]
    fn parses_frontmatter_into_metadata_and_body() {
        let (metadata, body) = HtmlParser::new()
            .parse("---\ntitle: hi\n---\n<p>body</p>")
            .unwrap();

        assert_eq!(
            metadata.inner().get("title"),
            Some(&serde_json::json!("hi"))
        );
        assert_eq!(body.inner(), "<p>body</p>");
    }

    #[test]
    fn rejects_invalid_frontmatter() {
        assert!(parse_yaml_frontmatter("title: [unclosed").is_err());
    }

    #[test]
    fn renders_markdown_into_html() {
        let parser = MarkdownParser::new(MarkdownParserOptions::default());

        let (metadata, body) = parser
            .parse("---\ntitle: hi\n---\n# Title\n\nSome *emphasis*.\n")
            .unwrap();

        assert_eq!(
            metadata.inner().get("title"),
            Some(&serde_json::json!("hi"))
        );
        assert!(body.inner().contains("<h1"));
        assert!(body.inner().contains("<em>emphasis</em>"));
    }

    #[test]
    fn keeps_raw_html_unless_disabled() {
        let markdown = "Some <b>bold</b> text\n";

        let (_, raw) = MarkdownParser::new(MarkdownParserOptions::default())
            .parse(markdown)
            .unwrap();
        assert!(raw.inner().contains("<b>bold</b>"));

        let (_, omitted) =
            MarkdownParser::new(MarkdownParserOptions::default().with_raw_html(false))
                .parse(markdown)
                .unwrap();
        assert!(omitted.inner().contains("<!-- raw HTML omitted -->"));
    }

    #[test]
    fn disables_automatic_heading_ids_when_requested() {
        let parser =
            MarkdownParser::new(MarkdownParserOptions::default().with_auto_heading_ids(false));

        let (_, body) = parser.parse("# Title\n").unwrap();

        assert!(body.inner().contains(">Title</h1>"));
        assert!(!body.inner().contains("id="));
    }

    #[test]
    fn disables_emojis_when_requested() {
        let markdown = "Hello :smile:\n";

        let (_, rendered) = MarkdownParser::new(MarkdownParserOptions::default())
            .parse(markdown)
            .unwrap();
        assert!(!rendered.inner().contains(":smile:"));

        let (_, literal) = MarkdownParser::new(MarkdownParserOptions::default().with_emojis(false))
            .parse(markdown)
            .unwrap();
        assert!(literal.inner().contains(":smile:"));
    }

    #[test]
    fn parses_json_metadata_and_content() {
        let input = r#"{"metadata": {"title": "hi", "views": 42}, "content": {"items": ["a"]}}"#;

        let (metadata, body) = JsonParser.parse(input).unwrap();

        assert_eq!(
            metadata.inner().get("title"),
            Some(&serde_json::json!("hi"))
        );
        assert_eq!(metadata.inner().get("views"), Some(&serde_json::json!(42)));
        assert_eq!(
            metadata.inner().get(VAR_CONTENT),
            Some(&serde_json::json!({"items": ["a"]}))
        );
        assert_eq!(body.inner(), "");
    }

    #[test]
    fn accepts_json_without_metadata_or_content() {
        let (metadata, body) = JsonParser.parse("{}").unwrap();

        assert!(metadata.inner().is_empty());
        assert_eq!(body.inner(), "");
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(JsonParser.parse("{").is_err());
    }
}
