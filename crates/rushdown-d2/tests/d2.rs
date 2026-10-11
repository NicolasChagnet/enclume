//! End-to-end checks of the extension through rushdown's own pipeline.
//!
//! The drawing is d2's own, so the tests assert what a reader of the page
//! gets — an inline SVG which carries both appearances — rather than the
//! shape of every path inside it.

use rushdown::new_markdown_to_html;
use rushdown_d2::{Options, d2_html_renderer_extension, d2_parser_extension};

const DIAGRAM: &str = "```d2\na -> b: hello\n```\n";

/// The `d2` command is a build tool of the machine which builds the page: a
/// machine without it skips the checks which would need it, rather than
/// reporting a failure its reader cannot fix
fn d2_is_installed() -> bool {
    std::process::Command::new("d2")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn render(markdown: &str) -> String {
    render_with(Options::default(), markdown)
}

fn render_with(options: Options, markdown: &str) -> String {
    html_with(options, markdown).unwrap()
}

fn html_with(options: Options, markdown: &str) -> rushdown::Result<String> {
    let markdown_to_html = new_markdown_to_html(
        Default::default(),
        Default::default(),
        d2_parser_extension(),
        d2_html_renderer_extension(options),
    );

    let mut html = String::new();
    markdown_to_html(&mut html, markdown)?;
    Ok(html)
}

#[test]
fn draws_a_d2_block_as_inline_svg() {
    if !d2_is_installed() {
        eprintln!("d2 is not installed, skipping test");
        return;
    }

    let html = render(DIAGRAM);

    assert!(
        html.contains(r#"<figure class="d2"><div><svg"#),
        "the diagram sits in the figure of a d2 diagram: {html}"
    );
    assert!(
        html.contains("</div></figure>"),
        "the figure closes: {html}"
    );
    assert!(
        !html.contains("<?xml"),
        "an SVG written into a page carries no XML declaration: {html}"
    );
    assert!(
        !html.contains("language-d2"),
        "the block is a diagram, not a code block: {html}"
    );
}

#[test]
fn writes_the_two_appearances_as_the_sides_of_light_dark() {
    if !d2_is_installed() {
        eprintln!("d2 is not installed, skipping test");
        return;
    }

    let html = render(DIAGRAM);

    assert!(
        !html.contains("prefers-color-scheme"),
        "the appearance of the diagram follows the color scheme of the page, not the system: {html}"
    );
    assert!(
        html.contains("light-dark(#0A0F25,#CDD6F4)"),
        "one colour holds both appearances: {html}"
    );
}

#[test]
fn draws_the_appearances_with_the_themes_the_options_give() {
    if !d2_is_installed() {
        eprintln!("d2 is not installed, skipping test");
        return;
    }

    let one_theme = render_with(
        Options::default().with_dark_theme(Options::DEFAULT_THEME),
        DIAGRAM,
    );
    let two_themes = render(DIAGRAM);

    assert_ne!(
        one_theme, two_themes,
        "the theme of the dark appearance reaches the command"
    );
}

#[test]
fn leaves_the_code_blocks_of_other_languages_alone() {
    let html = render("```rust\nlet answer = 42;\n```\n");

    assert!(html.contains("language-rust"), "{html}");
    assert!(!html.contains("<svg"), "{html}");
}

#[test]
fn reports_a_command_which_cannot_be_run() {
    let error = html_with(
        Options::default().with_command("d2-that-is-not-installed"),
        DIAGRAM,
    )
    .unwrap_err();

    assert!(
        format!("{error}").contains("d2 could not draw the diagram"),
        "unexpected error: {error}"
    );
}

#[test]
fn reports_a_diagram_the_command_cannot_draw() {
    if !d2_is_installed() {
        eprintln!("d2 is not installed, skipping test");
        return;
    }

    let error = html_with(Options::default(), "```d2\na ->\n```\n").unwrap_err();

    assert!(
        format!("{error}").contains("d2 could not draw the diagram"),
        "unexpected error: {error}"
    );
}
