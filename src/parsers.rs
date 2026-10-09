use crate::path::SitePath;
use rushdown::{parser::ParserExtension, renderer::html::RendererExtension};
use serde::{Deserialize, Serialize};
use snafu::prelude::*;
use std::path::PathBuf;

pub type Value = serde_json::Value;
pub type VarMap = std::collections::HashMap<String, Value>;

#[derive(Debug, Clone, Serialize)]
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

impl From<String> for RawHtml {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

/// Frontmatter as declared by the source file
///
/// The template path is validated during the conversion into [`ParsedData`].
#[derive(Debug, Clone, Deserialize)]
pub struct RawMetadata {
    pub template: PathBuf,
    pub vars: Option<VarMap>,
}

impl RawMetadata {
    pub fn new(template: PathBuf, vars: Option<VarMap>) -> Self {
        Self { template, vars }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ParsedData {
    #[serde(skip_serializing)]
    template: SitePath,
    vars: VarMap,
    __content__: Value,
}

impl ParsedData {
    pub fn new(template: SitePath, vars: VarMap, __content__: Value) -> Self {
        Self {
            template,
            vars,
            __content__,
        }
    }

    pub fn from_raw(raw_metadata: RawMetadata, content: Value) -> Result<Self, snafu::Whatever> {
        let template = SitePath::try_from(raw_metadata.template)?;
        Ok(Self {
            template,
            vars: raw_metadata.vars.unwrap_or_default(),
            __content__: content,
        })
    }

    pub fn template(&self) -> &SitePath {
        &self.template
    }

    pub fn as_value(&self) -> Result<Value, snafu::Whatever> {
        let var =
            serde_json::to_value(self).whatever_context("Could not build template parameters")?;
        Ok(var)
    }
}

pub trait ContentParser {
    /// Parse a string into metadata
    ///
    /// Returns `None` when the content declares no frontmatter, which marks the
    /// file for a verbatim copy instead of a rendering pass.
    fn parse(&self, content: &str) -> Result<Option<ParsedData>, snafu::Whatever>;
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
///
/// Inserts the rendered markdown inside the reserved `__content__` template variable.
#[derive(Clone, Debug, Default)]
pub struct MarkdownParser {
    options: MarkdownParserOptions,
}

impl MarkdownParser {
    pub fn new(options: MarkdownParserOptions) -> Self {
        Self { options }
    }
}

impl ContentParser for MarkdownParser {
    fn parse(&self, content: &str) -> Result<Option<ParsedData>, snafu::Whatever> {
        let (frontmatter, body) = split_yaml_frontmatter(content);
        if frontmatter.trim().is_empty() {
            return Ok(None);
        }
        let raw_metadata = parse_yaml_frontmatter(frontmatter)?;

        let parser = rushdown::new_markdown_to_html(
            self.options.parser_options(),
            self.options.renderer_options(),
            self.options.parser_extensions(),
            self.options.renderer_extensions(),
        );

        let mut parsed_body = String::new();
        if let Err(e) = parser(&mut parsed_body, body) {
            // rushdown::Error is not Send, so anyhow cannot keep it as a source
            whatever!("Could not parse Markdown content: {e}")
        }
        let parsed_data = ParsedData::from_raw(raw_metadata, parsed_body.into())?;
        Ok(Some(parsed_data))
    }
}

/// JSON content parser
///
/// Inserts the parsed JSON object inside the reserved `__content__` template variable.
#[derive(Debug, Clone, Default)]
pub struct JsonParser;

impl ContentParser for JsonParser {
    fn parse(&self, content: &str) -> Result<Option<ParsedData>, snafu::Whatever> {
        let (frontmatter, body) = split_yaml_frontmatter(content);
        if frontmatter.trim().is_empty() {
            return Ok(None);
        }
        let raw_metadata = parse_yaml_frontmatter(frontmatter)?;

        let parsed_json =
            serde_json::from_str(body).whatever_context("Could not parse JSON content")?;
        let parsed_data = ParsedData::from_raw(raw_metadata, parsed_json)?;
        Ok(Some(parsed_data))
    }
}

/// HTML content parser
///
/// Inserts the body inside the reserved `__content__` template variable, without
/// converting it.
#[derive(Debug, Clone, Default)]
pub struct HtmlParser;

impl ContentParser for HtmlParser {
    fn parse(&self, content: &str) -> Result<Option<ParsedData>, snafu::Whatever> {
        let (frontmatter, body) = split_yaml_frontmatter(content);
        if frontmatter.trim().is_empty() {
            return Ok(None);
        }
        let raw_metadata = parse_yaml_frontmatter(frontmatter)?;
        let parsed_data = ParsedData::from_raw(raw_metadata, body.into())?;
        Ok(Some(parsed_data))
    }
}

/// YAML content parser
///
/// Should be of the same format as a generic frontmatter
#[derive(Debug, Clone, Default)]
pub struct YamlParser;

impl ContentParser for YamlParser {
    fn parse(&self, content: &str) -> Result<Option<ParsedData>, snafu::Whatever> {
        let (frontmatter, body) = split_yaml_frontmatter(content);
        // We expect there to NOT be a frontmatter here
        if !frontmatter.trim().is_empty() {
            return Ok(None);
        }
        let raw_metadata = parse_yaml_frontmatter(body)?;
        let parsed_data = ParsedData::from_raw(raw_metadata, "".into())?;
        Ok(Some(parsed_data))
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
fn parse_yaml_frontmatter(frontmatter: &str) -> Result<RawMetadata, snafu::Whatever> {
    let map: RawMetadata = yaml_serde::from_str(frontmatter)
        .with_whatever_context(|e| format!("Couldn't deserialize frontmatter (error: {e})"))?;
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Rendered body of a Markdown string which declares a template
    fn rendered_markdown(parser: &MarkdownParser, markdown: &str) -> String {
        let metadata = parser
            .parse(markdown)
            .unwrap()
            .expect("expected frontmatter");
        metadata.as_value().unwrap()["__content__"]
            .as_str()
            .expect("expected rendered Markdown HTML")
            .to_string()
    }

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
    fn skips_markdown_without_frontmatter() {
        let parsed = MarkdownParser::default().parse("# Just content\n").unwrap();

        assert!(parsed.is_none());
    }

    #[test]
    fn handles_crlf_frontmatter() {
        let (frontmatter, body) =
            split_yaml_frontmatter("---\r\ntitle: hi\r\n---\r\n<p>body</p>\r\n");
        assert_eq!(frontmatter, "title: hi");
        assert_eq!(body, "<p>body</p>\r\n");
    }

    #[test]
    fn parses_frontmatter_into_metadata_and_content() {
        let metadata = MarkdownParser::default()
            .parse("---\ntemplate: post.html\nvars:\n  title: hi\n---\nbody")
            .unwrap()
            .unwrap();

        assert_eq!(metadata.template().as_path(), PathBuf::from("post.html"));
        let value = metadata.as_value().unwrap();
        // The reserved template key stays out of the template context
        assert!(value.get("template").is_none());
        assert_eq!(value["vars"]["title"], serde_json::json!("hi"));
        assert_eq!(value["__content__"], serde_json::json!("<p>body</p>\n"));
    }

    #[test]
    fn rejects_content_without_a_template() {
        let error = format!(
            "{:#}",
            MarkdownParser::default()
                .parse("---\nvars:\n  title: hi\n---\nbody")
                .unwrap_err()
        );

        assert!(error.contains("template"), "unexpected error: {error}");
    }

    #[test]
    fn rejects_a_template_path_which_escapes_the_site() {
        let error = format!(
            "{:#}",
            MarkdownParser::default()
                .parse("---\ntemplate: ../post.html\nvars: {}\n---\nbody")
                .unwrap_err()
        );

        assert!(error.contains("../post.html"), "unexpected error: {error}");
    }

    #[test]
    fn rejects_invalid_frontmatter() {
        assert!(parse_yaml_frontmatter("title: [unclosed").is_err());
    }

    #[test]
    fn renders_markdown_into_html() {
        let parser = MarkdownParser::new(MarkdownParserOptions::default());

        let content = rendered_markdown(
            &parser,
            "---\ntemplate: post.html\nvars:\n  title: hi\n---\n# Title\n\nSome *emphasis*.\n",
        );

        assert!(content.contains("<h1"));
        assert!(content.contains("<em>emphasis</em>"));
    }

    #[test]
    fn keeps_raw_html_unless_disabled() {
        let markdown = "---\ntemplate: post.html\nvars: {}\n---\nSome <b>bold</b> text\n";

        let raw = rendered_markdown(&MarkdownParser::default(), markdown);
        assert!(raw.contains("<b>bold</b>"));

        let omitted = rendered_markdown(
            &MarkdownParser::new(MarkdownParserOptions::default().with_raw_html(false)),
            markdown,
        );
        assert!(omitted.contains("<!-- raw HTML omitted -->"));
    }

    #[test]
    fn disables_automatic_heading_ids_when_requested() {
        let parser =
            MarkdownParser::new(MarkdownParserOptions::default().with_auto_heading_ids(false));

        let body = rendered_markdown(
            &parser,
            "---\ntemplate: post.html\nvars: {}\n---\n# Title\n",
        );

        assert!(body.contains(">Title</h1>"));
        assert!(!body.contains("id="));
    }

    #[test]
    fn disables_emojis_when_requested() {
        let markdown = "---\ntemplate: post.html\nvars: {}\n---\nHello :smile:\n";

        let rendered = rendered_markdown(&MarkdownParser::default(), markdown);
        assert!(!rendered.contains(":smile:"));

        let literal = rendered_markdown(
            &MarkdownParser::new(MarkdownParserOptions::default().with_emojis(false)),
            markdown,
        );
        assert!(literal.contains(":smile:"));
    }

    #[test]
    fn parses_json_metadata_and_content() {
        let input =
            "---\ntemplate: post.html\nvars:\n  title: hi\n  views: 42\n---\n{\"items\": [\"a\"]}";

        let metadata = JsonParser.parse(input).unwrap().unwrap();

        assert_eq!(metadata.template().as_path(), PathBuf::from("post.html"));
        let value = metadata.as_value().unwrap();
        assert_eq!(value["vars"]["title"], serde_json::json!("hi"));
        assert_eq!(value["vars"]["views"], serde_json::json!(42));
        assert_eq!(value["__content__"], serde_json::json!({"items": ["a"]}));
    }

    #[test]
    fn skips_json_without_frontmatter() {
        let parsed = JsonParser.parse(r#"{"items": ["a"]}"#).unwrap();

        assert!(parsed.is_none());
    }

    #[test]
    fn rejects_content_without_a_frontmatter_body() {
        assert!(
            JsonParser
                .parse("---\ntemplate: post.html\nvars: {}\n---\n")
                .is_err()
        );
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(
            JsonParser
                .parse("---\ntemplate: post.html\nvars: {}\n---\n{")
                .is_err()
        );
    }

    #[test]
    fn parses_html_metadata_and_content() {
        let input = "---\ntemplate: post.html\nvars:\n  title: hi\n---\n<h1>Raw</h1>\n";

        let metadata = HtmlParser.parse(input).unwrap().unwrap();

        assert_eq!(metadata.template().as_path(), PathBuf::from("post.html"));
        let value = metadata.as_value().unwrap();
        assert_eq!(value["vars"]["title"], serde_json::json!("hi"));
        assert_eq!(value["__content__"], serde_json::json!("<h1>Raw</h1>\n"));
    }

    #[test]
    fn skips_html_without_frontmatter() {
        let parsed = HtmlParser.parse("<p>no frontmatter</p>\n").unwrap();

        assert!(parsed.is_none());
    }
}
