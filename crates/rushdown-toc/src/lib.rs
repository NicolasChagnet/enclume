//! Collect the headings of a rushdown document into a table of contents.
//!
//! The entries are read from the parsed document rather than from its Markdown
//! source, so each one links to the very id the renderer wrote on its heading:
//!
//! ```
//! use rushdown::{
//!     parser::{Options, Parser},
//!     text::BasicReader,
//! };
//!
//! let source = "# Title\n\n## Installing\n\n### From source\n";
//! let parser = Parser::with_options(Options {
//!     auto_heading_ids: true,
//!     ..Default::default()
//! });
//! let mut reader = BasicReader::new(source);
//! let (arena, document) = parser.parse(&mut reader);
//!
//! let toc = rushdown_toc::table_of_contents(&arena, document, source);
//! let titles: Vec<_> = toc.iter().map(|entry| entry.title.as_str()).collect();
//! assert_eq!(titles, ["Title", "Installing", "From source"]);
//! ```
//!
//! The entries come in document order, and each one holds the level of its
//! heading, the id a link to it ends with, and its title. The title is the text
//! of the heading with the inline markup flattened: emphasis, strong, links and
//! code spans read as the prose they hold. An inline node which holds no prose
//! of its own, such as a formula, leaves no trace in the title.
//!
//! A heading inside a literal block is not a heading, and never becomes an
//! entry. A heading which carries no id is left out, since nothing can link to
//! it; automatic heading ids give every heading one.

use std::convert::Infallible;

use rushdown::{
    as_kind_data,
    ast::{Arena, KindData, NodeRef, TextQualifier, WalkStatus, walk},
    matches_kind,
};
use serde::Serialize;

/// One heading of a document, as an entry of its table of contents
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TocEntry {
    /// The level of the heading: `1` for `#`, `2` for `##`, and so on
    pub level: u8,

    /// The id of the heading, which a link to it ends with: `#installing`
    pub id: String,

    /// The text of the heading, with its inline markup flattened
    pub title: String,
}

/// Collect the headings of a parsed document, in document order
///
/// The arena and the source are the ones the document was parsed from, so
/// collect the entries while both are still at hand.
pub fn table_of_contents(arena: &Arena, document: NodeRef, source: &str) -> Vec<TocEntry> {
    let mut entries = Vec::new();

    let _ = walk(
        arena,
        document,
        &mut |arena: &Arena, node_ref: NodeRef, entering: bool| {
            if entering && matches_kind!(arena[node_ref], Heading) {
                let heading = as_kind_data!(arena[node_ref], Heading);
                if let Some(id) = id_of(arena, node_ref, source) {
                    entries.push(TocEntry {
                        level: heading.level(),
                        id,
                        title: title_of(arena, node_ref, source),
                    });
                }
            }

            Ok::<WalkStatus, Infallible>(WalkStatus::Continue)
        },
    );

    entries
}

/// The id of a heading, which an anchor link points at
///
/// Automatic ids are written by the parser, unless the author wrote one, so a
/// heading holds either the slug of its text or the id it was given.
fn id_of(arena: &Arena, heading: NodeRef, source: &str) -> Option<String> {
    arena[heading]
        .attributes()
        .get("id")
        .map(|id| id.str(source).into_owned())
}

/// The text of a heading, with its inline nodes flattened
fn title_of(arena: &Arena, heading: NodeRef, source: &str) -> String {
    let mut title = String::new();

    let _ = walk(
        arena,
        heading,
        &mut |arena: &Arena, node_ref: NodeRef, entering: bool| {
            if entering {
                match arena[node_ref].kind_data() {
                    KindData::Text(text) => {
                        title.push_str(text.str(source));

                        // The text of a line ends where the next line starts;
                        // neither carries the break itself
                        if text.has_qualifiers(TextQualifier::SOFT_LINE_BREAK)
                            || text.has_qualifiers(TextQualifier::HARD_LINE_BREAK)
                        {
                            title.push(' ');
                        }
                    }
                    KindData::CodeSpan(code) => title.push_str(&code.str(source)),
                    _ => {}
                }
            }

            Ok::<WalkStatus, Infallible>(WalkStatus::Continue)
        },
    );

    // A heading may span lines, but its title is written on one
    title.split_whitespace().collect::<Vec<_>>().join(" ")
}
