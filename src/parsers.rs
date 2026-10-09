use crate::path::SitePath;
use rushdown::{parser::ParserExtension, renderer::html::RendererExtension};
use snafu::prelude::*;
use std::path::PathBuf;

// Reserved variables
pub const VAR_TEMPLATE: &str = "template";
pub const VAR_CONTENT: &str = "__content__";

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
pub struct ParsedMetadata {
    template: SitePath,
    variables: VarMap,
}

impl ParsedMetadata {
    pub fn new(template: SitePath, variables: VarMap) -> Self {
        Self {
            template,
            variables,
        }
    }

    pub fn template(&self) -> &SitePath {
        &self.template
    }

    pub fn variables(&self) -> &VarMap {
        &self.variables
    }

    pub fn into_attrs(self) -> (SitePath, VarMap) {
        (self.template, self.variables)
    }
}

pub trait ContentParser {
    /// Parse a string into metadata
    ///
    /// Returns `None` when the content declares no frontmatter, which marks the
    /// file for a verbatim copy instead of a rendering pass.
    fn parse(&self, content: &str) -> Result<Option<ParsedMetadata>, snafu::Whatever>;
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
/// Inserts the rendered markdown inside the [`VAR_CONTENT`] variable of the templating variables.
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
    fn parse(&self, content: &str) -> Result<Option<ParsedMetadata>, snafu::Whatever> {
        let (frontmatter, body) = split_yaml_frontmatter(content);
        if frontmatter.trim().is_empty() {
            return Ok(None);
        }
        let mut var_map = parse_yaml_frontmatter(frontmatter)?;
        let template = pop_template_from_metadata(&mut var_map)?;

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
        var_map.insert(VAR_CONTENT.to_string(), parsed_body.into());
        Ok(Some(ParsedMetadata::new(template, var_map)))
    }
}

/// JSON content parser
///
/// Inserts the JSON object inside the [`VAR_CONTENT`] key of the templating variables.
pub struct JsonParser;

impl ContentParser for JsonParser {
    fn parse(&self, content: &str) -> Result<Option<ParsedMetadata>, snafu::Whatever> {
        let (frontmatter, body) = split_yaml_frontmatter(content);
        if frontmatter.trim().is_empty() {
            return Ok(None);
        }
        let mut var_map = parse_yaml_frontmatter(frontmatter)?;
        let template = pop_template_from_metadata(&mut var_map)?;

        let parsed_json =
            serde_json::from_str(body).whatever_context("Could not parse JSON content")?;
        var_map.insert(VAR_CONTENT.to_string(), parsed_json);
        Ok(Some(ParsedMetadata::new(template, var_map)))
    }
}

/// HTML content parser
///
/// Inserts the body inside the [`VAR_CONTENT`] variable of the templating
/// variables, without converting it.
pub struct HtmlParser;

impl ContentParser for HtmlParser {
    fn parse(&self, content: &str) -> Result<Option<ParsedMetadata>, snafu::Whatever> {
        let (frontmatter, body) = split_yaml_frontmatter(content);
        if frontmatter.trim().is_empty() {
            return Ok(None);
        }
        let mut var_map = parse_yaml_frontmatter(frontmatter)?;
        let template = pop_template_from_metadata(&mut var_map)?;

        var_map.insert(VAR_CONTENT.to_string(), body.into());
        Ok(Some(ParsedMetadata::new(template, var_map)))
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
fn parse_yaml_frontmatter(frontmatter: &str) -> Result<VarMap, snafu::Whatever> {
    let map = yaml_serde::from_str(frontmatter)
        .with_whatever_context(|e| format!("Couldn't deserialize frontmatter (error: {e})"))?;
    Ok(map)
}

/// Take the template name out of the metadata
///
/// Every content file must name the template that renders it, so the variable
/// is consumed here instead of reaching the template context.
fn pop_template_from_metadata(metadata: &mut VarMap) -> Result<SitePath, snafu::Whatever> {
    let template = metadata
        .remove(VAR_TEMPLATE)
        .and_then(|value| value.as_str().map(PathBuf::from))
        .whatever_context(format!(
            "No {VAR_TEMPLATE:?} variable in the content metadata"
        ))?;
    Ok(SitePath::new(template))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rendered body of a Markdown string which declares a template
    fn rendered_markdown(parser: &MarkdownParser, markdown: &str) -> String {
        let metadata = parser
            .parse(markdown)
            .unwrap()
            .expect("expected frontmatter");
        metadata
            .variables()
            .get(VAR_CONTENT)
            .and_then(|content| content.as_str())
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
            .parse("---\ntitle: hi\ntemplate: post.html\n---\nbody")
            .unwrap()
            .unwrap();

        assert_eq!(metadata.template().as_path(), PathBuf::from("post.html"));
        assert_eq!(
            metadata.variables().get("title"),
            Some(&serde_json::json!("hi"))
        );
        assert_eq!(
            metadata.variables().get(VAR_CONTENT),
            Some(&serde_json::json!("<p>body</p>\n"))
        );
    }

    #[test]
    fn consumes_the_template_variable() {
        let metadata = MarkdownParser::default()
            .parse("---\ntemplate: post.html\n---\nbody")
            .unwrap()
            .unwrap();

        assert!(!metadata.variables().contains_key(VAR_TEMPLATE));
    }

    #[test]
    fn rejects_content_without_a_template() {
        let error = format!(
            "{:#}",
            MarkdownParser::default()
                .parse("---\ntitle: hi\n---\nbody")
                .unwrap_err()
        );

        assert!(error.contains(VAR_TEMPLATE), "unexpected error: {error}");
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
            "---\ntitle: hi\ntemplate: post.html\n---\n# Title\n\nSome *emphasis*.\n",
        );

        assert!(content.contains("<h1"));
        assert!(content.contains("<em>emphasis</em>"));
    }

    #[test]
    fn keeps_raw_html_unless_disabled() {
        let markdown = "---\ntemplate: post.html\n---\nSome <b>bold</b> text\n";

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

        let body = rendered_markdown(&parser, "---\ntemplate: post.html\n---\n# Title\n");

        assert!(body.contains(">Title</h1>"));
        assert!(!body.contains("id="));
    }

    #[test]
    fn disables_emojis_when_requested() {
        let markdown = "---\ntemplate: post.html\n---\nHello :smile:\n";

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
        let input = "---\ntemplate: post.html\ntitle: hi\nviews: 42\n---\n{\"items\": [\"a\"]}";

        let metadata = JsonParser.parse(input).unwrap().unwrap();

        assert_eq!(metadata.template().as_path(), PathBuf::from("post.html"));
        assert_eq!(
            metadata.variables().get("title"),
            Some(&serde_json::json!("hi"))
        );
        assert_eq!(
            metadata.variables().get("views"),
            Some(&serde_json::json!(42))
        );
        assert_eq!(
            metadata.variables().get(VAR_CONTENT),
            Some(&serde_json::json!({"items": ["a"]}))
        );
    }

    #[test]
    fn skips_json_without_frontmatter() {
        let parsed = JsonParser.parse(r#"{"items": ["a"]}"#).unwrap();

        assert!(parsed.is_none());
    }

    #[test]
    fn rejects_content_without_a_frontmatter_body() {
        assert!(JsonParser.parse("---\ntemplate: post.html\n---\n").is_err());
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(
            JsonParser
                .parse("---\ntemplate: post.html\n---\n{")
                .is_err()
        );
    }

    #[test]
    fn parses_html_metadata_and_content() {
        let input = "---\ntemplate: post.html\ntitle: hi\n---\n<h1>Raw</h1>\n";

        let metadata = HtmlParser.parse(input).unwrap().unwrap();

        assert_eq!(metadata.template().as_path(), PathBuf::from("post.html"));
        assert_eq!(
            metadata.variables().get("title"),
            Some(&serde_json::json!("hi"))
        );
        assert_eq!(
            metadata.variables().get(VAR_CONTENT),
            Some(&serde_json::json!("<h1>Raw</h1>\n"))
        );
    }

    #[test]
    fn skips_html_without_frontmatter() {
        let parsed = HtmlParser.parse("<p>no frontmatter</p>\n").unwrap();

        assert!(parsed.is_none());
    }
}
