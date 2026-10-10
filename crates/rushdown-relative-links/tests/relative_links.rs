//! End-to-end checks of the extension through rushdown's own pipeline.

use rushdown::new_markdown_to_html;
use rushdown_relative_links::relative_links_extension;

/// Render `markdown` with the extension resolving against `base`
fn render(base: &str, markdown: &str) -> String {
    let parser = new_markdown_to_html(
        Default::default(),
        Default::default(),
        relative_links_extension(base),
        rushdown::renderer::html::empty_renderer_extension(),
    );

    let mut html = String::new();
    parser(&mut html, markdown).unwrap();
    html
}

#[test]
fn resolves_relative_images_and_links() {
    let html = render(
        "/blog/post",
        "![plot](plot.png)\n\n[the manual](manual.pdf)\n",
    );

    assert!(html.contains(r#"src="/blog/post/plot.png""#), "{html}");
    assert!(html.contains(r#"href="/blog/post/manual.pdf""#), "{html}");
}

#[test]
fn keeps_destinations_which_are_already_resolved() {
    let html = render(
        "/blog/post",
        "![a](/blog/post/plot.png) ![b](https://example.com/plot.png) [c](#anchor)\n",
    );

    assert!(html.contains(r#"src="/blog/post/plot.png""#), "{html}");
    assert!(
        html.contains(r#"src="https://example.com/plot.png""#),
        "{html}"
    );
    assert!(html.contains(r##"href="#anchor""##), "{html}");
}

#[test]
fn keeps_titles_and_reference_definitions() {
    let html = render(
        "/blog/post",
        "![plot](plot.png \"A plot\")\n\n[ref][definition]\n\n[definition]: spec.pdf\n",
    );

    assert!(
        html.contains(r#"<img src="/blog/post/plot.png" alt="plot" title="A plot">"#),
        "{html}"
    );
    assert!(html.contains(r#"href="/blog/post/spec.pdf""#), "{html}");
    assert!(html.contains(r#">ref</a>"#), "{html}");
}

#[test]
fn resolves_nothing_in_code() {
    let html = render("/blog/post", "```\n![plot](plot.png)\n```\n");

    assert!(html.contains("![plot](plot.png)"), "{html}");
    assert!(!html.contains("/blog/post/plot.png"), "{html}");
}
