//! End-to-end checks of the extension through rushdown's own pipeline.

use rushdown::new_markdown_to_html;
use rushdown_math::{math_html_renderer_extension, math_parser_extension};

/// Render `markdown` with both halves of the extension
fn render(markdown: &str) -> String {
    let parser = new_markdown_to_html(
        Default::default(),
        Default::default(),
        math_parser_extension(),
        math_html_renderer_extension(),
    );

    let mut html = String::new();
    parser(&mut html, markdown).unwrap();
    html
}

#[test]
fn draws_inline_math_beside_the_text() {
    let html = render("Energy is $E = mc^2$, as everyone knows.\n");

    assert!(
        html.contains(r#"<p>Energy is <span class="math math-inline"><span class="katex">"#),
        "{html}"
    );
    assert!(html.contains(", as everyone knows.</p>"), "{html}");
    assert!(
        html.contains(r#"<annotation encoding="application/x-tex">E = mc^2</annotation>"#),
        "{html}"
    );
}

#[test]
fn draws_display_math_in_a_paragraph() {
    let html = render("$$\n\\int_0^1 x\\,dx = \\frac{1}{2}\n$$\n");

    assert!(
        html.contains(r#"<span class="math math-display"><span class="katex-display">"#),
        "{html}"
    );
}

#[test]
fn draws_a_math_block_as_a_block() {
    let html = render("```math\n\\int_0^1 x\\,dx = \\frac{1}{2}\n```\n");

    assert!(
        html.contains(r#"<div class="math math-display"><span class="katex-display">"#),
        "{html}"
    );
    assert!(html.contains("</span></div>"), "{html}");
}

#[test]
fn draws_math_inside_the_other_blocks() {
    let html = render("# Sums $x$\n\n- $a_1^2$\n\n> $\\pi$\n");

    assert!(html.contains(r#"<h1"#), "{html}");
    assert!(
        html.contains(r#"><span class="math math-inline">"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<li><span class="math math-inline">"#),
        "{html}"
    );
    assert!(html.contains(r#"<blockquote>"#), "{html}");
    assert_eq!(html.matches(r#"<span class="katex">"#).count(), 3, "{html}");
}

#[test]
fn leaves_prose_dollars_alone() {
    let html = render("It cost $5 and then $10 more, or \\$3.\n");

    assert!(!html.contains("katex"), "{html}");
    assert!(
        html.contains("<p>It cost $5 and then $10 more, or $3.</p>"),
        "{html}"
    );
}

#[test]
fn looks_for_a_closing_dollar_in_the_same_block_only() {
    let html = render("An opening $ alone.\n\nA closing one$ here.\n");

    assert!(!html.contains("katex"), "{html}");
    assert!(html.contains("An opening $ alone."), "{html}");
    assert!(html.contains("A closing one$ here."), "{html}");
}

#[test]
fn draws_inline_math_across_lines() {
    let html = render("A formula $a +\nb$ which wraps.\n");

    assert!(
        html.contains(r#"A formula <span class="math math-inline"><span class="katex">"#),
        "{html}"
    );
    assert!(html.contains("which wraps."), "{html}");
}

#[test]
fn draws_display_math_across_lines() {
    let html = render("$$\na + b\n= c\n$$\n\nafter\n");

    assert!(
        html.contains(r#"<span class="math math-display"><span class="katex-display">"#),
        "{html}"
    );
    assert!(html.contains("<p>after</p>"), "{html}");
}

#[test]
fn leaves_math_looking_text_inside_code_alone() {
    let html = render("`$x$`\n\n```\n$x$\n```\n");

    assert!(!html.contains("katex"), "{html}");
    assert!(html.contains("<code>$x$</code>"), "{html}");
}

#[test]
fn shows_a_formula_which_does_not_parse() {
    let html = render("$\\frac{$\n");

    assert!(html.contains(r#"<span class="katex-error""#), "{html}");
    assert!(
        html.contains("KaTeX parse error: Unexpected end of input"),
        "{html}"
    );
    assert!(html.contains(r">\frac{</span>"), "{html}");
}
