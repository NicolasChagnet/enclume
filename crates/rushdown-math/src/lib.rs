//! Render LaTeX math in a rushdown document as KaTeX markup.
//!
//! Formulas are typeset by [katex-rs](https://github.com/katex-rs/katex-rs), a
//! port of [KaTeX](https://katex.org) written in Rust, while the document is
//! rendered. The markup is the markup of KaTeX itself: HTML laid out by its
//! stylesheet, and a MathML copy which carries the source of the formula.
//!
//! Three notations are recognised:
//!
//! - `$…$` sets a formula inline, in the flow of a sentence;
//! - `$$…$$` sets display math, on a line of its own or in the middle of one;
//! - a `math` fenced block sets display math, one line of LaTeX per line.
//!
//! ```no_run
//! use rushdown::new_markdown_to_html;
//!
//! let parser = new_markdown_to_html(
//!     Default::default(),
//!     Default::default(),
//!     rushdown_math::math_parser_extension(),
//!     rushdown_math::math_html_renderer_extension(),
//! );
//!
//! let mut html = String::new();
//! parser(&mut html, "Energy is $E = mc^2$.").unwrap();
//! assert!(html.contains(r#"<span class=\"katex\">"#));
//! ```
//!
//! KaTeX lays a formula out with CSS, so the page must load KaTeX's stylesheet
//! and the fonts it points at. Every formula carries the classes `math` and
//! `math-inline` or `math-display` around KaTeX's own markup, so that a page
//! can style formulas without depending on class names internal to KaTeX.
//!
//! A dollar sign which opens no formula is left alone: `$5 and $10` is prose,
//! because an inline formula neither starts nor ends on a space. A formula
//! which does not parse is drawn in red with the reason for it in a tooltip,
//! as KaTeX does, so that one bad formula does not fail a whole build.

use std::{any::TypeId, convert::Infallible, fmt, marker::PhantomData, sync::OnceLock};

use katex::{KatexContext, OutputFormat, ParseError, Settings, render_to_string};
use rushdown::{
    as_extension_data, as_kind_data, as_type_data, as_type_data_mut,
    ast::{Arena, KindData, NodeKind, NodeRef, NodeType, PrettyPrint, WalkStatus, pp_indent, walk},
    matches_kind,
    parser::{
        AnyAstTransformer, AnyInlineParser, AstTransformer, Context, InlineParser, NoParserOptions,
        ParserExtension, parser_extension,
    },
    renderer::{
        self, BoxRenderNode, NoRendererOptions, NodeRenderer, NodeRendererRegistry, RenderNode,
        TextWrite,
        html::{self, Renderer, RendererExtension, RendererExtensionFn},
    },
    text::{BasicReader, BlockReader, Reader as _},
};

// KaTeX {{{

/// The engine, built once and shared by every document
///
/// The context holds the symbol tables, the functions and the font metrics of
/// KaTeX. Building it costs far more than rendering with it does.
static CONTEXT: OnceLock<KatexContext> = OnceLock::new();

/// Typeset `latex` into the HTML and MathML markup KaTeX produces
///
/// The error is what the engine answered when it refused the formula outright.
/// A formula which merely fails to parse comes back as the red `katex-error`
/// markup KaTeX writes for it.
fn render_math(latex: &str, kind: MathKind) -> Result<String, ParseError> {
    let settings = Settings::builder()
        .display_mode(kind == MathKind::Display)
        .output(OutputFormat::HtmlAndMathml)
        .throw_on_error(false)
        .build();

    render_to_string(CONTEXT.get_or_init(KatexContext::default), latex, &settings)
}

// }}} KaTeX

// Extensions {{{

/// Returns a parser extension recognising `$…$`, `$$…$$` and `math` blocks
pub fn math_parser_extension() -> impl ParserExtension {
    /// Before emphasis, so that a formula is consumed whole
    const PRIORITY: u32 = 90;

    parser_extension(move |parser| {
        parser.add_inline_parser(MathInlineParser::new, NoParserOptions, PRIORITY);
        parser.add_ast_transformer(MathAstTransformer::new, NoParserOptions, PRIORITY);
    })
}

/// Returns an HTML renderer extension writing formulas as KaTeX markup
pub fn math_html_renderer_extension<'cb, W>() -> impl RendererExtension<'cb, W>
where
    W: TextWrite + 'cb,
{
    RendererExtensionFn::new(move |renderer: &mut Renderer<'cb, W>| {
        renderer.add_node_renderer(MathHtmlRenderer::new, NoRendererOptions);
    })
}

// }}} Extensions

// AST {{{

/// The way a formula is set
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MathKind {
    /// In the flow of a sentence, from `$…$`
    Inline,
    /// Apart from the flow, from `$$…$$` or a `math` block
    Display,
}

/// A LaTeX formula in the AST
#[derive(Debug)]
struct Math {
    kind: MathKind,
    /// Whether the formula starts a block, from a `math` fenced block
    ///
    /// Display math written `$$…$$` sits inside a paragraph, and stays an
    /// inline node: a block element would cut that paragraph in two.
    block: bool,
    value: String,
}

impl Math {
    /// A formula written `$…$` or `$$…$$` inside a paragraph
    fn paragraph(value: String, kind: MathKind) -> Self {
        Self {
            kind,
            block: false,
            value,
        }
    }

    /// A display formula written as a fenced block
    fn block(value: String) -> Self {
        Self {
            kind: MathKind::Display,
            block: true,
            value,
        }
    }

    /// The LaTeX source of the formula
    fn value(&self) -> &str {
        &self.value
    }

    /// The class carried by the element of the formula
    fn class(&self) -> &'static str {
        match self.kind {
            MathKind::Inline => "math-inline",
            MathKind::Display => "math-display",
        }
    }

    /// The element the formula is written as
    fn element(&self) -> &'static str {
        match self.block {
            true => "div",
            false => "span",
        }
    }
}

impl NodeKind for Math {
    fn typ(&self) -> NodeType {
        match self.block {
            true => NodeType::LeafBlock,
            false => NodeType::Inline,
        }
    }

    fn kind_name(&self) -> &'static str {
        "Math"
    }
}

impl PrettyPrint for Math {
    fn pretty_print(&self, w: &mut dyn fmt::Write, _source: &str, level: usize) -> fmt::Result {
        writeln!(w, "{}Formula: {}", pp_indent(level), self.value())?;
        writeln!(
            w,
            "{}Display: {}",
            pp_indent(level),
            self.kind == MathKind::Display
        )
    }
}

impl From<Math> for KindData {
    fn from(math: Math) -> Self {
        KindData::Extension(Box::new(math))
    }
}

// }}} AST

// Parser {{{

/// Parses `$…$` and `$$…$$` formulas
#[derive(Debug)]
struct MathInlineParser;

impl MathInlineParser {
    fn new() -> Self {
        Self
    }
}

impl InlineParser for MathInlineParser {
    fn trigger(&self) -> &[u8] {
        b"$"
    }

    fn parse(
        &self,
        arena: &mut Arena,
        _parent_ref: NodeRef,
        reader: &mut BlockReader,
        _ctx: &mut Context,
    ) -> Option<NodeRef> {
        let source = reader.source();
        let bytes = source.as_bytes();
        let (line, segment) = reader.peek_line_bytes()?;
        let start = segment.start();

        // A formula opens on `$$` for display math and on `$` for inline math.
        // The reader never sits on a padding byte, so the opener is the first
        // byte of the line, and the byte after it tells which of the two it is.
        let (opener, kind) = match line.get(1) {
            Some(b'$') => (2, MathKind::Display),
            // `$ x$` and a lone `$` are prose, not math
            Some(next) if !next.is_ascii_whitespace() => (1, MathKind::Inline),
            _ => return None,
        };
        reader.advance(opener);

        // Look for the closing delimiter line by line: an inline parser reads
        // past the current line, but must not read past the end of its block.
        loop {
            let (_, segment) = reader.peek_line_bytes()?;
            let (line_start, line_stop) = (segment.start(), segment.stop());
            let padding = segment.padding();
            let line = &bytes[line_start..line_stop];

            let mut i = 0;
            while i < line.len() {
                // The opener guarantees a byte before any delimiter
                let previous = bytes[line_start + i - 1];
                if line[i] != b'$' || previous == b'\\' {
                    i += 1;
                    continue;
                }
                let closer = match opener {
                    // `$5 and $10` is prose: an inline formula cannot close
                    // after a space
                    1 if !previous.is_ascii_whitespace() => 1,
                    2 if line.get(i + 1) == Some(&b'$') => 2,
                    _ => {
                        i += 1;
                        continue;
                    }
                };

                let value = source[start + opener..line_start + i].to_string();
                reader.advance(padding + i + closer);
                return Some(arena.new_node(Math::paragraph(value, kind)));
            }

            reader.advance_line();
        }
    }
}

impl From<MathInlineParser> for AnyInlineParser {
    fn from(parser: MathInlineParser) -> Self {
        AnyInlineParser::Extension(Box::new(parser))
    }
}

/// Replaces `math` fenced blocks with display formulas
#[derive(Debug)]
struct MathAstTransformer;

impl MathAstTransformer {
    fn new() -> Self {
        Self
    }
}

impl AstTransformer for MathAstTransformer {
    fn transform(
        &self,
        arena: &mut Arena,
        document: NodeRef,
        reader: &mut BasicReader,
        _context: &mut Context,
    ) {
        let source = reader.source();
        let mut fenced: Vec<NodeRef> = Vec::new();
        let _ = walk(
            arena,
            document,
            &mut |arena: &Arena, node_ref: NodeRef, entering: bool| {
                if entering && matches_kind!(arena[node_ref], CodeBlock) {
                    let code_block = as_kind_data!(arena[node_ref], CodeBlock);
                    if code_block.language_str(source) == Some("math") {
                        fenced.push(node_ref);
                    }
                }
                Ok::<WalkStatus, Infallible>(WalkStatus::Continue)
            },
        );

        for code_ref in fenced {
            let value: String = as_kind_data!(arena[code_ref], CodeBlock)
                .value()
                .iter(source)
                .collect();
            let parent = arena[code_ref].parent();
            let position = arena[code_ref].pos();
            let blank = as_type_data!(arena, code_ref, Block).has_blank_previous_line();

            let math_ref = arena.new_node(Math::block(value));
            if let Some(position) = position {
                arena[math_ref].set_pos(position);
            }
            as_type_data_mut!(arena, math_ref, Block).set_blank_previous_line(blank);

            if let Some(parent) = parent {
                parent.replace_child(arena, code_ref, math_ref);
            }
        }
    }
}

impl From<MathAstTransformer> for AnyAstTransformer {
    fn from(transformer: MathAstTransformer) -> Self {
        AnyAstTransformer::Extension(Box::new(transformer))
    }
}

// }}} Parser

// Renderer {{{

/// Writes a formula as KaTeX markup
struct MathHtmlRenderer<W: TextWrite> {
    _phantom: PhantomData<W>,
    writer: html::Writer,
}

impl<W: TextWrite> MathHtmlRenderer<W> {
    fn new(html_options: html::Options) -> Self {
        Self {
            _phantom: PhantomData,
            writer: html::Writer::with_options(html_options),
        }
    }
}

impl<W: TextWrite> RenderNode<W> for MathHtmlRenderer<W> {
    fn render_node<'a>(
        &self,
        w: &mut W,
        _source: &'a str,
        arena: &'a Arena,
        node_ref: NodeRef,
        entering: bool,
        _ctx: &mut renderer::Context,
    ) -> rushdown::Result<WalkStatus> {
        if entering {
            let math = as_extension_data!(arena, node_ref, Math);
            let (element, class) = (math.element(), math.class());

            // KaTeX escapes what it writes, so its markup goes through as is
            match render_math(math.value(), math.kind) {
                Ok(markup) => self.writer.write_html(
                    w,
                    &format!(r#"<{element} class="math {class}">{markup}</{element}>"#),
                )?,
                // Only the engine refusing the formula has to be written by hand
                Err(error) => {
                    self.writer.write_html(
                        w,
                        &format!(r#"<{element} class="math {class} math-error">"#),
                    )?;
                    self.writer.raw_write(w, &error.to_string())?;
                    self.writer.write_html(w, &format!("</{element}>"))?;
                }
            }
        }
        Ok(WalkStatus::Continue)
    }
}

impl<'cb, W> NodeRenderer<'cb, W> for MathHtmlRenderer<W>
where
    W: TextWrite + 'cb,
{
    fn register_node_renderer_fn(self, nrr: &mut impl NodeRendererRegistry<'cb, W>) {
        nrr.register_node_renderer_fn(TypeId::of::<Math>(), BoxRenderNode::new(self));
    }
}

// }}} Renderer

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typesets_a_formula_into_html_and_mathml() {
        let markup = render_math("E = mc^2", MathKind::Inline).unwrap();

        assert!(markup.starts_with(r#"<span class="katex">"#), "{markup}");
        assert!(
            markup.contains(r#"<annotation encoding="application/x-tex">E = mc^2</annotation>"#),
            "{markup}"
        );
    }

    #[test]
    fn sets_a_display_formula_apart() {
        let markup = render_math(r"\int_0^1 x\,dx", MathKind::Display).unwrap();

        assert!(
            markup.starts_with(r#"<span class="katex-display">"#),
            "{markup}"
        );
    }

    #[test]
    fn writes_a_formula_which_does_not_parse_in_red() {
        let markup = render_math(r"\frac{", MathKind::Inline).unwrap();

        assert!(markup.contains(r#"<span class="katex-error""#), "{markup}");
        assert!(markup.contains(r">\frac{</span>"), "{markup}");
    }
}
