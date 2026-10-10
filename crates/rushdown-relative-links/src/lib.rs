//! Resolve the relative destinations of a document against a base directory.
//!
//! A page is usually reachable at more than one URL: `/blog/post` and `/blog/post/`
//! both serve the same file. A browser resolves `plot.png` against the *document
//! URL*, not against where the file sits, so the two spellings point at different
//! places — and which spelling a visitor gets depends on the server's redirect
//! policy. Rewriting the destination to a root-relative one removes that
//! dependency, so the same output works behind `try_files`, behind a server that
//! redirects to the trailing slash, and when the layout changes.
//!
//! ```no_run
//! use rushdown::new_markdown_to_html;
//!
//! let parser = new_markdown_to_html(
//!     Default::default(),
//!     Default::default(),
//!     rushdown_relative_links::relative_links_extension("/blog/post"),
//!     rushdown::renderer::html::empty_renderer_extension(),
//! );
//!
//! let mut html = String::new();
//! parser(&mut html, "![plot](plot.png)").unwrap();
//! assert!(html.contains(r#"src="/blog/post/plot.png""#));
//! ```
//!
//! Relative destinations are resolved as *file* references, so a document at
//! `blog/post/index.md` with a sibling `plot.png` refers to `/blog/post/plot.png`.
//! A relative link to another page cannot be resolved without knowing that page's
//! own URL, so link to pages with a root-relative or absolute destination.
//!
//! Destinations which need no resolving are left alone: root-relative,
//! protocol-relative, fragment-only, query-only, and anything carrying a scheme
//! (`https:`, `mailto:`, `data:`, …). Autolinks are skipped: they are URLs by
//! construction, whether or not they spell out their scheme.

use std::convert::Infallible;

use rushdown::{
    as_kind_data,
    ast::{Arena, Image, KindData, Link, LinkKind, NodeRef, WalkStatus, walk},
    matches_kind,
    parser::{AnyAstTransformer, AstTransformer, Context, ParserOptions, parser_extension},
    text::{BasicReader, Reader as _},
};

/// Options of the extension
#[derive(Debug, Clone)]
pub struct RelativeLinksOptions {
    /// The root-relative directory a document's relative destinations resolve against
    ///
    /// Empty or `/` for a document sitting at the root of the site.
    pub base: String,
}

impl RelativeLinksOptions {
    /// Options resolving relative destinations against `base`
    pub fn new(base: impl Into<String>) -> Self {
        Self { base: base.into() }
    }
}

impl From<String> for RelativeLinksOptions {
    fn from(base: String) -> Self {
        Self::new(base)
    }
}

impl From<&str> for RelativeLinksOptions {
    fn from(base: &str) -> Self {
        Self::new(base)
    }
}

impl ParserOptions for RelativeLinksOptions {}

/// Returns a parser extension resolving the relative destinations of a document
///
/// The base directory is a property of the document, not of the parser, so build
/// one extension per document: `parser_extensions().and(relative_links_extension(base))`.
pub fn relative_links_extension(
    options: impl Into<RelativeLinksOptions>,
) -> impl rushdown::parser::ParserExtension {
    /// Runs on the completed document, after the syntax extensions
    const PRIORITY: u32 = 1000;

    let options = options.into();
    parser_extension(move |parser| {
        parser.add_ast_transformer(RelativeLinks::with_options, options.clone(), PRIORITY);
    })
}

/// Rewrites the relative destinations of a document
#[derive(Debug)]
struct RelativeLinks {
    base: String,
}

impl RelativeLinks {
    fn with_options(options: RelativeLinksOptions) -> Self {
        Self { base: options.base }
    }

    /// The destination to write, when it is relative
    fn resolved(&self, destination: &str) -> Option<String> {
        resolve(&self.base, destination)
    }
}

impl AstTransformer for RelativeLinks {
    fn transform(
        &self,
        arena: &mut Arena,
        document: NodeRef,
        reader: &mut BasicReader,
        _context: &mut Context,
    ) {
        let source = reader.source();
        let mut rewrites: Vec<(NodeRef, String)> = Vec::new();
        let _ = walk(
            arena,
            document,
            &mut |arena: &Arena, node_ref: NodeRef, entering: bool| {
                if entering {
                    if matches_kind!(arena[node_ref], Image) {
                        let image = as_kind_data!(arena[node_ref], Image);
                        if let Some(destination) = self.resolved(image.destination_str(source)) {
                            rewrites.push((node_ref, destination));
                        }
                    } else if matches_kind!(arena[node_ref], Link) {
                        let link = as_kind_data!(arena[node_ref], Link);
                        // Autolinks are URLs: a bare `www.example.com` must not
                        // turn into a document-relative path
                        if !matches!(link.link_kind(), LinkKind::Auto(_))
                            && let Some(destination) = self.resolved(link.destination_str(source))
                        {
                            rewrites.push((node_ref, destination));
                        }
                    }
                }
                Ok::<WalkStatus, Infallible>(WalkStatus::Continue)
            },
        );

        for (node_ref, destination) in rewrites {
            let kind_data = arena[node_ref].kind_data_mut();
            let rebuilt = match &mut *kind_data {
                KindData::Image(image) => KindData::Image(resolved_image(image, destination)),
                KindData::Link(link) => KindData::Link(resolved_link(link, destination)),
                _ => continue,
            };
            *kind_data = rebuilt;
        }
    }
}

impl From<RelativeLinks> for AnyAstTransformer {
    fn from(transformer: RelativeLinks) -> Self {
        AnyAstTransformer::Extension(Box::new(transformer))
    }
}

/// The same image, pointing at `destination`
fn resolved_image(image: &Image, destination: String) -> Image {
    let title = image.title().cloned();
    match image.link_kind() {
        LinkKind::Reference(reference) => {
            let value = reference.value().clone();
            let kind = reference.link_reference_kind();
            match title {
                Some(title) => Image::reference_with_title(destination, value, kind, title),
                None => Image::reference(destination, value, kind),
            }
        }
        _ => match title {
            Some(title) => Image::inline_with_title(destination, title),
            None => Image::inline(destination),
        },
    }
}

/// The same link, pointing at `destination`
fn resolved_link(link: &Link, destination: String) -> Link {
    let title = link.title().cloned();
    match link.link_kind() {
        LinkKind::Reference(reference) => {
            let value = reference.value().clone();
            let kind = reference.link_reference_kind();
            match title {
                Some(title) => Link::reference_with_title(destination, value, kind, title),
                None => Link::reference(destination, value, kind),
            }
        }
        _ => match title {
            Some(title) => Link::inline_with_title(destination, title),
            None => Link::inline(destination),
        },
    }
}

/// Resolve a relative destination against `base`
///
/// Returns `None` for the destinations a build must leave alone: empty,
/// root-relative, protocol-relative, fragment-only, query-only, or carrying a
/// scheme (`https:`, `mailto:`, `data:`, …).
fn resolve(base: &str, destination: &str) -> Option<String> {
    if destination.is_empty()
        || destination.starts_with('/')
        || destination.starts_with('#')
        || destination.starts_with('?')
        || has_scheme(destination)
    {
        return None;
    }
    let joined = match base.is_empty() {
        true => format!("/{destination}"),
        false => format!("{base}/{destination}"),
    };
    Some(normalize(&joined))
}

/// Whether a destination opens with a URL scheme, which makes it absolute
fn has_scheme(destination: &str) -> bool {
    let Some((scheme, _)) = destination.split_once(':') else {
        return false;
    };
    scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// Resolve `.` and `..` segments, keeping the result root-relative
fn normalize(destination: &str) -> String {
    let mut segments: Vec<&str> = Vec::new();
    for segment in destination.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            segment => segments.push(segment),
        }
    }
    format!("/{}", segments.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_destinations_against_the_base() {
        assert_eq!(
            resolve("/blog/post", "plot.png").unwrap(),
            "/blog/post/plot.png"
        );
        assert_eq!(
            resolve("/blog/post", "./plot.png").unwrap(),
            "/blog/post/plot.png"
        );
        assert_eq!(
            resolve("/blog", "post/plot.png").unwrap(),
            "/blog/post/plot.png"
        );
        // A document at the root of the site
        assert_eq!(resolve("", "plot.png").unwrap(), "/plot.png");
        assert_eq!(resolve("/", "plot.png").unwrap(), "/plot.png");
        // `..` walks back up and cannot escape the root
        assert_eq!(
            resolve("/blog/a/b", "../plot.png").unwrap(),
            "/blog/a/plot.png"
        );
        assert_eq!(resolve("/blog", "../../plot.png").unwrap(), "/plot.png");
        // Queries and fragments stay attached to the file they qualify
        assert_eq!(
            resolve("/blog/post", "plot.png?v=2#top").unwrap(),
            "/blog/post/plot.png?v=2#top"
        );
    }

    #[test]
    fn leaves_destinations_the_browser_resolves_alone() {
        for destination in [
            "/blog/post/plot.png",          // already root-relative
            "https://example.com/plot.png", // absolute
            "http://example.com/plot.png",
            "//example.com/plot.png", // protocol-relative
            "mailto:someone@example.com",
            "data:image/png;base64,AAAA",
            "tel:+31000000000",
            "#section",
            "?query=1",
            "",
        ] {
            assert!(
                resolve("/blog/post", destination).is_none(),
                "{destination} should be left alone"
            );
        }
    }
}
