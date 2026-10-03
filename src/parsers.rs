pub type StringMap = std::collections::HashMap<String, String>;

#[derive(Debug, Clone)]
pub struct RawHtml(String);

impl RawHtml {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    pub fn inner(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

#[derive(Debug, Clone)]
pub struct ParsedMetadata(StringMap);

impl ParsedMetadata {
    pub fn new(value: StringMap) -> Self {
        Self(value)
    }

    pub fn inner(&self) -> &StringMap {
        &self.0
    }

    pub fn into_inner(self) -> StringMap {
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
fn parse_yaml_frontmatter(frontmatter: &str) -> anyhow::Result<StringMap> {
    let map = yaml_serde::from_str(frontmatter)?;
    Ok(map)
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

        assert_eq!(metadata.inner().get("title"), Some(&"hi".to_string()));
        assert_eq!(body.inner(), "<p>body</p>");
    }

    #[test]
    fn rejects_invalid_frontmatter() {
        assert!(parse_yaml_frontmatter("title: [unclosed").is_err());
    }
}
