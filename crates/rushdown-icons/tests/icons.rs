//! End-to-end checks of the extension through rushdown's own pipeline.
//!
//! The emoji extension is in the pipeline as well, since the two share the `:`
//! trigger and a site runs both.

use rushdown::parser::{Options, ParserExtension};
use rushdown::renderer::html::{self, RendererExtension};
use rushdown_emoji::{
    EmojiHtmlRendererOptions, EmojiParserOptions, emoji_html_renderer_extension,
    emoji_parser_extension,
};
use rushdown_icons::{
    IconsHtmlRendererOptions, icons_html_renderer_extension, icons_parser_extension,
};

/// The HTML of `markdown`, rendered by default
fn html(markdown: &str) -> String {
    render(markdown, html::Options::default())
}

/// The HTML of `markdown`, rendered with `renderer_options`
fn render(markdown: &str, renderer_options: html::Options) -> String {
    render_icons(
        markdown,
        renderer_options,
        IconsHtmlRendererOptions::default(),
    )
}

/// The HTML of `markdown`, rendered with options of both extensions
fn render_icons(
    markdown: &str,
    renderer_options: html::Options,
    icon_options: IconsHtmlRendererOptions,
) -> String {
    let convert =
        rushdown::new_markdown_to_html_string(
            Options::default(),
            renderer_options,
            icons_parser_extension().and(emoji_parser_extension(EmojiParserOptions::default())),
            icons_html_renderer_extension::<String>(icon_options).and(
                emoji_html_renderer_extension::<String>(EmojiHtmlRendererOptions::default()),
            ),
        );

    let mut out = String::new();
    convert(&mut out, markdown).expect("expected the markdown to render");
    out
}

/// The first `<path>` element of a rendered icon, which draws it
fn first_path(rendered: &str) -> &str {
    let start = rendered.find("<path").expect("expected a path");
    let end = rendered[start..]
        .find("/>")
        .expect("expected a closed path");

    &rendered[start..start + end + 2]
}

#[test]
fn renders_every_always_on_set_as_an_inline_svg() {
    for markdown in [
        "::lucide:arrow-left::",
        "::octicon:git-branch::",
        "::si:rust::",
    ] {
        let rendered = html(markdown);
        let icon = first_path(&rendered);

        assert!(
            rendered.contains(r#"width="1em" height="1em""#),
            "{rendered}"
        );
        assert!(!icon.contains("width=\"24\""), "{rendered}");
        assert!(icon.len() > "<path d=\"\"/>".len(), "{rendered}");
    }
}

#[test]
fn an_icon_names_its_set_and_its_name() {
    let rendered = html("::lucide:arrow-left::");

    assert!(
        rendered.contains(r#"class="icon icon-lucide-arrow-left""#),
        "{rendered}"
    );
    assert!(rendered.contains(r#"aria-hidden="true""#), "{rendered}");
    assert!(rendered.contains(r#"focusable="false""#), "{rendered}");
}

#[test]
fn a_site_can_restyle_the_icons_of_a_page() {
    let rendered = render_icons(
        "::lucide:arrow-left::",
        html::Options::default(),
        IconsHtmlRendererOptions {
            class: "prose-icon".to_string(),
            ..Default::default()
        },
    );

    assert!(
        rendered.contains(r#"class="prose-icon prose-icon-lucide-arrow-left""#),
        "{rendered}"
    );
}

#[test]
fn an_octicon_needs_no_size_suffix() {
    let plain = html("::octicon:git-branch::");
    let sized = html("::octicon:git-branch-24::");

    assert_eq!(first_path(&plain), first_path(&sized));
    assert!(plain.contains("icon-octicon-git-branch"), "{plain}");
    assert!(sized.contains("icon-octicon-git-branch-24"), "{sized}");
}

#[test]
fn declines_an_unknown_set_or_name_as_text() {
    let rendered = html("::nope:arrow-left:: and ::lucide:no-such-icon::");

    assert!(rendered.contains("::nope:arrow-left::"), "{rendered}");
    assert!(rendered.contains("::lucide:no-such-icon::"), "{rendered}");
    assert!(!rendered.contains("<svg"), "{rendered}");
}

#[test]
fn leaves_an_emoji_alone() {
    // The emoji extension still reads the shortcode through the leading colon
    let rendered = html("::rocket::");

    assert!(!rendered.contains("<svg"), "{rendered}");
    assert!(rendered.contains('🚀'), "{rendered}");
}

#[test]
fn leaves_a_timestamp_alone() {
    assert!(html("Meet at 10:30 sharp.\n").contains("10:30"));
}

#[test]
fn escapes_a_construct() {
    let rendered = html(r"\::lucide:arrow-left::");

    assert!(rendered.contains("::lucide:arrow-left::"), "{rendered}");
    assert!(!rendered.contains("<svg"), "{rendered}");
}

#[test]
fn a_code_span_holds_literal_text() {
    let rendered = html("`::lucide:zap::`\n");

    assert!(
        rendered.contains("<code>::lucide:zap::</code>"),
        "{rendered}"
    );
}

#[test]
fn renders_inside_a_block() {
    let rendered = html("# ::lucide:zap:: Deploy\n");

    assert!(rendered.contains("<h1>"), "{rendered}");
    assert!(rendered.contains("<svg"), "{rendered}");
    assert!(rendered.contains(" Deploy</h1>"), "{rendered}");
}

#[test]
fn an_icon_is_not_author_html() {
    let rendered = render(
        "::lucide:arrow-left::",
        html::Options {
            allows_unsafe: false,
            ..Default::default()
        },
    );

    assert!(rendered.contains("<svg"), "{rendered}");
}

#[test]
fn a_visible_icon_keeps_its_place_in_the_accessibility_tree() {
    let rendered = render_icons(
        "::lucide:arrow-left::",
        html::Options::default(),
        IconsHtmlRendererOptions {
            decorative: false,
            ..Default::default()
        },
    );

    assert!(!rendered.contains("aria-hidden"), "{rendered}");
    assert!(rendered.contains("<svg"), "{rendered}");
}

#[cfg(feature = "fontawesome")]
#[test]
fn renders_the_font_awesome_styles() {
    for markdown in [
        "::fa:rocket::",
        "::fa-solid:rocket::",
        "::fa-regular:bell::",
        "::fa-brands:github::",
    ] {
        let rendered = html(markdown);

        assert!(rendered.contains("<svg"), "{rendered}");
        assert!(rendered.contains(r#"width="1em""#), "{rendered}");
    }
}

#[cfg(not(feature = "fontawesome"))]
#[test]
fn font_awesome_is_out_of_the_default_build() {
    let rendered = html("::fa:arrow-up::");

    assert!(rendered.contains("::fa:arrow-up::"), "{rendered}");
    assert!(!rendered.contains("<svg"), "{rendered}");
}
