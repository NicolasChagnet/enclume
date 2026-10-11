//! End-to-end checks of the extension through rushdown's own pipeline.
//!
//! The markup is lumis's own, so the tests assert what a reader of the page
//! sees — the language, the lines, the colours of the theme — rather than the
//! shape of every element.

use rushdown::new_markdown_to_html;
use rushdown_lumis::{Options, Theme, lumis_html_renderer_extension};

const DARK: &str = "catppuccin_mocha";
const LIGHT: &str = "catppuccin_latte";

fn theme(name: &str) -> Theme {
    name.parse().unwrap()
}

fn options() -> Options {
    Options::new(theme(DARK))
}

fn render(markdown: &str) -> String {
    render_with(options(), markdown)
}

fn render_with(options: Options, markdown: &str) -> String {
    let parser = new_markdown_to_html(
        Default::default(),
        Default::default(),
        rushdown::parser::empty_parser_extension(),
        lumis_html_renderer_extension(options),
    );

    let mut html = String::new();
    parser(&mut html, markdown).unwrap();
    html
}

#[test]
fn writes_a_block_with_the_colors_of_the_theme() {
    let html = render("```rust\nlet answer = 42;\n```\n");

    assert!(html.starts_with("<pre class=\"lumis\""), "{html}");
    assert!(
        html.contains(r#"style="color: #cdd6f4; background-color: #1e1e2e;""#),
        "the block wears the colors of the theme: {html}"
    );
    assert!(html.contains(r#"<code class="language-rust""#), "{html}");
    assert!(
        html.contains(r#"<span class="l-line" data-line="1">"#),
        "the block is written line by line: {html}"
    );
    assert!(
        html.contains(r#"<span style="color: #cba6f7;">let</span>"#),
        "a keyword wears the color of the theme: {html}"
    );
    assert!(html.ends_with("</code></pre>"), "{html}");
}

#[test]
fn writes_a_block_of_a_language_it_does_not_know_plain() {
    let html = render("```not-a-language\nlet answer = <42>;\n```\n");

    assert!(
        html.contains(r#"<code class="language-plaintext""#),
        "{html}"
    );
    assert!(
        html.contains(r#"<span class="l-line" data-line="1">let answer = &lt;42&gt;;</span>"#),
        "the code is escaped and set plain: {html}"
    );
}

#[test]
fn writes_a_block_without_a_language_plain() {
    let html = render("```\nplain\n```\n");

    assert!(
        html.contains(r#"<code class="language-plaintext""#),
        "{html}"
    );
    assert!(html.contains(r#">plain</span>"#), "{html}");
}

#[test]
fn draws_an_indented_block() {
    let html = render("    indented code\n");

    assert!(
        html.contains(r#"<code class="language-plaintext""#),
        "{html}"
    );
    assert!(html.contains("indented code"), "{html}");
}

#[test]
fn escapes_the_code() {
    let html = render("```text\n<p class=\"note\">&amp;</p>\n```\n");

    assert!(
        html.contains(
            r#"<span class="l-line" data-line="1">&lt;p class=&quot;note&quot;&gt;&amp;amp;&lt;/p&gt;</span>"#
        ),
        "the code is escaped: {html}"
    );
}

#[test]
fn writes_the_title_bar_of_a_block_which_has_one() {
    let html = render("```rust title=\"src/main.rs\"\nfn main() {}\n```\n");

    assert!(
        html.starts_with(
            r#"<figure class="lumis-figure"><figcaption class="lumis-title">src/main.rs</figcaption><pre class="lumis""#
        ),
        "the title bar holds the title and then the block: {html}"
    );
    assert!(html.ends_with("</pre></figure>"), "{html}");
}

#[test]
fn escapes_the_title_of_a_block() {
    let html = render("```rust title=\"a <b> & c\"\nfn main() {}\n```\n");

    assert!(
        html.contains(">a &lt;b&gt; &amp; c</figcaption>"),
        "the title is escaped: {html}"
    );
}

#[test]
fn numbers_the_lines_of_a_block_which_asks_for_it() {
    let html = render("```rust line_numbers\n// a comment\nlet answer = 42;\n```\n");

    assert!(
        html.contains(r#"<span class="l-line-number" style="-webkit-user-select: none; user-select: none; color: #45475a;" aria-hidden="true">1</span>"#),
        "a line number is written beside the line it numbers: {html}"
    );
    assert!(html.contains(r#"aria-hidden="true">2</span>"#), "{html}");
    assert!(html.contains(r#"data-line="2""#), "{html}");
}

#[test]
fn numbers_the_lines_of_every_block_when_the_options_say_so() {
    let html = render_with(
        options().with_line_numbers(true),
        "```\nfirst\n```\n\n```rust line_numbers=false\nsecond\n```\n",
    );

    assert_eq!(html.matches("l-line-number").count(), 1, "{html}");
    assert!(html.contains(r#">first</span>"#), "{html}");
}

#[test]
fn marks_the_lines_a_block_asks_for() {
    let html = render("```rust highlight_lines=1\n// a comment\nlet answer = 42;\n```\n");

    assert_eq!(
        html.matches("background-color: #2a2b3c").count(),
        1,
        "only the marked line wears the color of the theme for highlighted lines: {html}"
    );
    assert!(
        html.contains(r#"<span class="l-line l-highlighted""#),
        "the marked line carries the class lumis gives it: {html}"
    );
}

#[test]
fn names_the_scope_of_every_token_when_the_options_ask_for_it() {
    let html = render_with(
        options().with_highlights(true),
        "```rust\nlet answer = 42;\n```\n",
    );

    assert!(
        html.contains(r#"<span data-highlight="keyword" style="color: #cba6f7;">let</span>"#),
        "a token names the scope it was highlighted as: {html}"
    );
    assert!(
        !render("```rust\nlet answer = 42;\n```\n").contains("data-highlight"),
        "a page which does not ask for the names does not pay for them"
    );
}

#[test]
fn writes_italic_only_when_the_options_ask_for_it() {
    let plain = render("```rust\n// a comment\n```\n");
    let italic = render_with(options().with_italic(true), "```rust\n// a comment\n```\n");

    assert!(!plain.contains("font-style: italic"), "{plain}");
    assert!(
        italic.contains(r#"<span style="color: #9399b2; font-style: italic;">// a comment</span>"#),
        "{italic}"
    );
}

#[test]
fn writes_the_two_themes_of_a_page_read_in_both() {
    let html = render_with(
        options().with_theme(theme(LIGHT)),
        "```rust\nlet answer = 42;\n```\n",
    );

    assert!(
        html.contains(r#"<pre class="lumis lumis-themes dark light""#),
        "{html}"
    );
    assert!(
        html.contains(r#"<span style="color: light-dark(#8839ef, #cba6f7);">let</span>"#),
        "the two themes are the two sides of light-dark(): {html}"
    );
}

#[test]
fn writes_the_title_bar_of_a_block_written_in_both_themes() {
    let html = render_with(
        options().with_theme(theme(LIGHT)),
        "```rust title=\"src/main.rs\"\nlet answer = 42;\n```\n",
    );

    assert!(
        html.starts_with(
            r#"<figure class="lumis-figure"><figcaption class="lumis-title">src/main.rs</figcaption><pre class="lumis lumis-themes dark light""#
        ),
        "{html}"
    );
    assert!(html.ends_with("</pre></figure>"), "{html}");
}

#[test]
fn writes_one_theme_when_the_second_is_not_the_other_appearance() {
    let html = render_with(
        options().with_theme(theme("gruvbox_dark")),
        "```rust\nlet answer = 42;\n```\n",
    );

    assert!(!html.contains("light-dark("), "{html}");
    assert!(html.contains(r#"<pre class="lumis""#), "{html}");
    assert!(
        html.contains(r#"<span style="color: #cba6f7;">let</span>"#),
        "the first theme is the theme of the block: {html}"
    );
}
