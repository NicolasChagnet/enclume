//! Writing a code block with lumis.
//!
//! lumis owns the markup: it writes a `<pre>` holding a `<code>` whose lines
//! are `<span>` elements styled inline from the theme, with the colours of the
//! theme on the block itself, the gutter of line numbers in the colour the
//! theme gives to `line_number`, and a marked line in the colour it gives to
//! `highlighted` and with the class `l-highlighted`, the class lumis gives it
//! in its own markup. A block with a title is written inside the title bar of
//! the page — a `<figure>` holding the `<figcaption>` of the title — and a
//! block whose options ask for it names the scope of every token in the
//! `data-highlight` attribute of its `<span>`.

use std::{any::TypeId, collections::HashMap, io, marker::PhantomData, str::FromStr};

use lumis::{
    HtmlInlineBuilder, HtmlMultiThemesBuilder,
    formatter::html_inline::{HighlightLines, HighlightLinesStyle},
    html::{HtmlElement, escape},
    write_highlight,
};
use rushdown::{
    as_kind_data,
    ast::{Arena, CodeBlock, NodeRef, WalkStatus},
    renderer::{
        self, BoxRenderNode, NodeRenderer, NodeRendererRegistry, RenderNode, TextWrite, html,
    },
};

use crate::{
    Appearance, Language, Options, Theme,
    meta::{self, MetaOptions},
};

/// What a code block asks for, once its fence has been read
struct Block<'a> {
    language: Language,
    title: Option<&'a str>,
    line_numbers: bool,
    highlight_lines: Option<HighlightLines>,
    code: &'a str,
}

/// Writes a code block as lumis HTML
///
/// `language` and `meta` are the two parts of the info string of the fence,
/// and `code` is the source of the block, line endings and all.
fn render_block(options: &Options, language: &str, meta: &MetaOptions, code: &str) -> String {
    let marked = meta.highlight_lines();
    let block = Block {
        language: Language::from_str(language).unwrap_or(Language::PlainText),
        title: meta.title(),
        line_numbers: meta.flag("line_numbers").unwrap_or(options.line_numbers()),
        highlight_lines: (!marked.is_empty()).then_some(HighlightLines {
            lines: marked,
            style: Some(HighlightLinesStyle::Theme),
            // The class lumis gives a marked line itself, so that a page styles
            // one with the same rule whatever the formatter wrote it
            class: Some("l-highlighted".to_string()),
        }),
        code,
    };

    let mut output = Vec::new();
    if write_block(options, &block, &mut output).is_err() {
        // A language whose parser cannot be compiled, or whose query cannot be
        // run, is set plain, so that one bad language does not fail a build
        output.clear();
        let _ = write_plain(options, &block, &mut output);
    }

    String::from_utf8_lossy(&output).into_owned()
}

/// Writes a block with the formatter the options ask for
fn write_block(options: &Options, block: &Block<'_>, output: &mut Vec<u8>) -> io::Result<()> {
    match options.themes() {
        // Two themes of opposite appearance are written as the two sides of
        // the `light-dark()` function of CSS, which a browser picks from with
        // no stylesheet at all
        [first, second] if first.appearance != second.appearance => {
            let themes = HashMap::from([
                (appearance_name(first), first.clone()),
                (appearance_name(second), second.clone()),
            ]);
            let mut builder = HtmlMultiThemesBuilder::new();
            builder
                .language(block.language)
                .themes(themes)
                .default_theme("light-dark()")
                .italic(options.italic())
                .line_numbers(block.line_numbers)
                .include_highlights(options.highlights())
                .highlight_lines(block.highlight_lines.clone())
                .header(block.title.map(title_bar));

            let formatter = builder.build().map_err(io::Error::other)?;
            write_highlight(output, block.code, formatter)
        }
        themes => {
            let mut builder = HtmlInlineBuilder::new();
            builder
                .language(block.language)
                .theme(themes.first().cloned())
                .italic(options.italic())
                .line_numbers(block.line_numbers)
                .include_highlights(options.highlights())
                .highlight_lines(block.highlight_lines.clone())
                .header(block.title.map(title_bar));

            let formatter = builder.build().map_err(io::Error::other)?;
            write_highlight(output, block.code, formatter)
        }
    }
}

/// Writes a block no language styles, with the lines it asked for
fn write_plain(options: &Options, block: &Block<'_>, output: &mut Vec<u8>) -> io::Result<()> {
    let mut builder = HtmlInlineBuilder::new();
    builder
        .language(Language::PlainText)
        .theme(options.themes().first().cloned())
        .italic(options.italic())
        .line_numbers(block.line_numbers)
        .include_highlights(options.highlights())
        .highlight_lines(block.highlight_lines.clone())
        .header(block.title.map(title_bar));

    let formatter = builder.build().map_err(io::Error::other)?;
    write_highlight(output, block.code, formatter)
}

/// The title bar of a block which has a title
///
/// lumis writes the bar around the block as it writes the block, so the title
/// is escaped here, before the bar holds it.
fn title_bar(title: &str) -> HtmlElement {
    HtmlElement {
        open_tag: format!(
            r#"<figure class="lumis-figure"><figcaption class="lumis-title">{}</figcaption>"#,
            escape(title)
        ),
        close_tag: "</figure>".to_string(),
    }
}

/// The name a theme carries as a side of `light-dark()`
fn appearance_name(theme: &Theme) -> String {
    match theme.appearance {
        Appearance::Light => "light".to_string(),
        Appearance::Dark => "dark".to_string(),
    }
}

/// Renders a code block with lumis
pub(crate) struct LumisHtmlRenderer<W: TextWrite> {
    _phantom: PhantomData<W>,
    writer: html::Writer,
    options: Options,
}

impl<W: TextWrite> LumisHtmlRenderer<W> {
    pub(crate) fn new(html_options: html::Options, options: Options) -> Self {
        Self {
            _phantom: PhantomData,
            writer: html::Writer::with_options(html_options),
            options,
        }
    }
}

impl<W: TextWrite> RenderNode<W> for LumisHtmlRenderer<W> {
    fn render_node<'a>(
        &self,
        w: &mut W,
        source: &'a str,
        arena: &'a Arena,
        node_ref: NodeRef,
        entering: bool,
        _context: &mut renderer::Context,
    ) -> rushdown::Result<WalkStatus> {
        if entering {
            let code_block = as_kind_data!(arena, node_ref, CodeBlock);
            let code: String = code_block.value().iter(source).collect();
            let (language, meta) = meta::parts(code_block.info_str(source).unwrap_or_default());
            let meta = MetaOptions::parse(meta);

            let html = render_block(&self.options, language, &meta, &code);
            // lumis escapes the code it writes, so its markup goes through as is
            self.writer.write_html(w, &html)?;
        }

        Ok(WalkStatus::Continue)
    }
}

impl<'cb, W> NodeRenderer<'cb, W> for LumisHtmlRenderer<W>
where
    W: TextWrite + 'cb,
{
    fn register_node_renderer_fn(self, nrr: &mut impl NodeRendererRegistry<'cb, W>) {
        nrr.register_node_renderer_fn(TypeId::of::<CodeBlock>(), BoxRenderNode::new(self));
    }
}
