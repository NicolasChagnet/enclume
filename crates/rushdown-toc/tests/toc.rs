//! End-to-end checks of the extension through rushdown's own pipeline.

use rushdown::{
    parser::{Options, Parser},
    text::BasicReader,
};
use rushdown_toc::{TocEntry, table_of_contents};

/// The table of contents of `markdown`, parsed with automatic heading ids
fn toc(markdown: &str) -> Vec<TocEntry> {
    toc_with(
        markdown,
        Options {
            auto_heading_ids: true,
            ..Default::default()
        },
    )
}

/// The table of contents of `markdown`, parsed with `options`
fn toc_with(markdown: &str, options: Options) -> Vec<TocEntry> {
    let parser = Parser::with_options(options);
    let mut reader = BasicReader::new(markdown);
    let (arena, document) = parser.parse(&mut reader);

    table_of_contents(&arena, document, markdown)
}

/// An entry spelled out, for comparison
fn entry(level: u8, id: &str, title: &str) -> TocEntry {
    TocEntry {
        level,
        id: id.to_string(),
        title: title.to_string(),
    }
}

#[test]
fn collects_headings_in_document_order() {
    let entries = toc("# Title\n\
         \n\
         Some prose, then headings of every level.\n\
         \n\
         ## Installing\n\
         \n\
         ### From source\n");

    assert_eq!(
        entries,
        [
            entry(1, "title", "Title"),
            entry(2, "installing", "Installing"),
            entry(3, "from-source", "From source"),
        ]
    );
}

#[test]
fn reads_setext_headings_on_one_line() {
    let entries = toc("Title\n\
         =====\n\
         \n\
         Across\n\
         two lines\n\
         ----------\n");

    assert_eq!(
        entries,
        [
            entry(1, "title", "Title"),
            entry(2, "across-two-lines", "Across two lines"),
        ]
    );
}

#[test]
fn flattens_inline_markup_in_titles() {
    let titles: Vec<_> = toc("## Use **bold**, `code` and [a link](/manual.pdf)\n")
        .into_iter()
        .map(|entry| entry.title)
        .collect();

    assert_eq!(titles, ["Use bold, code and a link"]);
}

#[test]
fn numbers_headings_which_share_a_title() {
    let entries = toc("## Notes\n\n## Notes\n");

    assert_eq!(
        entries,
        [entry(2, "notes", "Notes"), entry(2, "notes-1", "Notes")]
    );
}

#[test]
fn keeps_an_id_written_on_the_heading() {
    let entries = toc_with(
        "## Installing {#setup}\n",
        Options {
            attributes: true,
            auto_heading_ids: true,
            ..Default::default()
        },
    );

    assert_eq!(entries, [entry(2, "setup", "Installing")]);
}

#[test]
fn ignores_headings_inside_literal_blocks() {
    let entries =
        toc("## Real heading\n\n```\n# Not a heading\n```\n\n    # Not a heading either\n");

    assert_eq!(entries, [entry(2, "real-heading", "Real heading")]);
}

#[test]
fn skips_headings_without_an_id() {
    let entries = toc_with("# Title\n", Options::default());

    assert!(entries.is_empty(), "{entries:?}");
}

#[test]
fn collects_nothing_from_a_document_without_headings() {
    let entries = toc("Just prose.\n");

    assert!(entries.is_empty(), "{entries:?}");
}
