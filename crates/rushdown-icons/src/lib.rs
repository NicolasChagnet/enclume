//! Inline icons for Markdown.
//!
//! A name written between double colons becomes an inline `<svg>`:
//!
//! ```
//! use rushdown::{parser::Options, renderer::html};
//! use rushdown_icons::{
//!     IconsHtmlRendererOptions, icons_html_renderer_extension, icons_parser_extension,
//! };
//!
//! let convert = rushdown::new_markdown_to_html_string(
//!     Options::default(),
//!     html::Options::default(),
//!     icons_parser_extension(),
//!     icons_html_renderer_extension::<String>(IconsHtmlRendererOptions::default()),
//! );
//!
//! let mut out = String::new();
//! convert(&mut out, "Press ::lucide:zap:: to deploy.\n").unwrap();
//! assert!(out.contains(r#"class="icon icon-lucide-zap""#));
//! ```
//!
//! The namespace comes first, and is mandatory: `::lucide:arrow-left::`,
//! `::octicon:git-branch::`, `::si:rust::`, and, behind the `fontawesome`
//! feature, `::fa:rocket::`, `::fa-regular:bell::` and `::fa-brands:github::`.
//! The icon takes the size and the colour of the text around it, so prose needs
//! no stylesheet and the site ships only the icons it uses.
//!
//! An unknown namespace or an unknown icon name leaves the text as it was
//! written, so `::rocket::` still belongs to the emoji extension, and a
//! timestamp such as `10:30` is untouched. `\::lucide:zap::` escapes the
//! construct.

use std::any::TypeId;
use std::fmt;
use std::fmt::Write;

use rushdown::ast::{
    Arena, KindData, NodeKind, NodeRef, NodeType, PrettyPrint, WalkStatus, pp_indent,
};
use rushdown::parser::{
    AnyInlineParser, InlineParser, NoParserOptions, PRIORITY_EMPHASIS, Parser, ParserExtension,
    ParserExtensionFn,
};
use rushdown::renderer::html::{self, Renderer, RendererExtension, RendererExtensionFn, Writer};
use rushdown::renderer::{
    self, BoxRenderNode, NodeRenderer, NodeRendererRegistry, RenderNode, RendererOptions, TextWrite,
};
use rushdown::{Result, as_extension_data};
use rushdown::{text, text::Reader};

mod sets;
mod svg;

// AST {{{
/// Represents an inline icon in the AST
#[derive(Debug)]
pub struct Icon {
    svg: &'static str,
    set: &'static str,
    name: String,
}

impl Icon {
    /// Returns the SVG document of the icon, as vendored
    pub fn svg(&self) -> &'static str {
        self.svg
    }

    /// Returns the namespace the icon was looked up in, `lucide` for instance
    pub fn set(&self) -> &'static str {
        self.set
    }

    /// Returns the name the icon was looked up by
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl NodeKind for Icon {
    fn typ(&self) -> NodeType {
        NodeType::Inline
    }

    fn kind_name(&self) -> &'static str {
        "Icon"
    }
}

impl PrettyPrint for Icon {
    fn pretty_print(&self, w: &mut dyn Write, _source: &str, level: usize) -> fmt::Result {
        writeln!(w, "{}set: {:?}", pp_indent(level), self.set)?;
        writeln!(w, "{}name: {:?}", pp_indent(level), self.name)
    }
}

impl From<Icon> for KindData {
    fn from(i: Icon) -> Self {
        KindData::Extension(Box::new(i))
    }
}
// }}} AST

// Parser {{{
#[derive(Debug)]
struct IconsParser;

impl InlineParser for IconsParser {
    fn trigger(&self) -> &[u8] {
        b":"
    }

    fn parse(
        &self,
        arena: &mut Arena,
        _parent_ref: NodeRef,
        reader: &mut text::BlockReader,
        _ctx: &mut rushdown::parser::Context,
    ) -> Option<NodeRef> {
        let (line, _) = reader.peek_line_bytes()?;
        let rest = line.strip_prefix(b"::")?;
        let (set, rest) = split_name(rest)?;
        let rest = rest.strip_prefix(b":")?;
        let (name, rest) = split_name(rest)?;
        let rest = rest.strip_prefix(b"::")?;

        let set = str::from_utf8(set).ok()?;
        let name = str::from_utf8(name).ok()?;
        let found = sets::lookup(set, name)?;

        reader.advance(line.len() - rest.len());
        Some(arena.new_node(Icon {
            svg: found.svg,
            set: found.set,
            name: name.to_string(),
        }))
    }
}

/// Split a namespace or an icon name off the front of `line`, up to the next `:`
///
/// Only the bytes a namespace or an icon name may hold are accepted, so nothing
/// else reaches the lookup, nor the class attribute the renderer writes. A
/// construct which does not parse declines without consuming, and stays text.
fn split_name(line: &[u8]) -> Option<(&[u8], &[u8])> {
    let end = line.iter().position(|&c| c == b':')?;
    let (name, rest) = line.split_at(end);

    name.iter()
        .all(|&c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_' || c == b'.')
        .then_some((name, rest))
}

impl From<IconsParser> for AnyInlineParser {
    fn from(p: IconsParser) -> Self {
        AnyInlineParser::Extension(Box::new(p))
    }
}
// }}} Parser

// Renderer {{{
/// Options for the icon HTML renderer
#[derive(Debug, Clone)]
pub struct IconsHtmlRendererOptions {
    /// Class put on every icon, and prefixed to the icon name for a second
    /// class which targets that one icon; "icon" by default
    pub class: String,

    /// Hide icons from assistive technology; true by default
    ///
    /// An icon stands for the words next to it, `::lucide:zap:: Deploy`, and a
    /// screen reader which reads both says the same thing twice.
    pub decorative: bool,
}

impl Default for IconsHtmlRendererOptions {
    fn default() -> Self {
        Self {
            class: "icon".to_string(),
            decorative: true,
        }
    }
}

impl RendererOptions for IconsHtmlRendererOptions {}

struct IconsHtmlRenderer<W: TextWrite> {
    _phantom: std::marker::PhantomData<W>,
    writer: Writer,
    options: IconsHtmlRendererOptions,
}

impl<W: TextWrite> IconsHtmlRenderer<W> {
    fn new(html_opts: html::Options, options: IconsHtmlRendererOptions) -> Self {
        Self {
            _phantom: std::marker::PhantomData,
            writer: Writer::with_options(html_opts),
            options,
        }
    }

    /// The classes an icon carries: the configured one, and one naming it
    fn class(&self, icon: &Icon) -> String {
        format!(
            "{class} {class}-{set}-{name}",
            class = self.options.class,
            set = icon.set,
            name = icon.name,
        )
    }
}

impl<W: TextWrite> RenderNode<W> for IconsHtmlRenderer<W> {
    fn render_node<'a>(
        &self,
        w: &mut W,
        _source: &'a str,
        arena: &'a Arena,
        node_ref: NodeRef,
        entering: bool,
        _context: &mut renderer::Context,
    ) -> Result<WalkStatus> {
        if entering {
            let icon = as_extension_data!(arena, node_ref, Icon);
            let class = self.class(icon);
            let svg = svg::inline_svg(icon.svg, &class, self.options.decorative);
            self.writer.write_html(w, &svg)?;
        }

        Ok(WalkStatus::Continue)
    }
}

impl<'cb, W> NodeRenderer<'cb, W> for IconsHtmlRenderer<W>
where
    W: TextWrite + 'cb,
{
    fn register_node_renderer_fn(self, nrr: &mut impl NodeRendererRegistry<'cb, W>) {
        nrr.register_node_renderer_fn(TypeId::of::<Icon>(), BoxRenderNode::new(self));
    }
}
// }}} Renderer

// Extension {{{
/// Returns a parser extension that resolves icons
pub fn icons_parser_extension() -> impl ParserExtension {
    ParserExtensionFn::new(|p: &mut Parser| {
        p.add_inline_parser(|| IconsParser, NoParserOptions, PRIORITY_EMPHASIS - 100);
    })
}

/// Returns a renderer extension that renders icons as inline `<svg>` elements
pub fn icons_html_renderer_extension<'cb, W>(
    options: IconsHtmlRendererOptions,
) -> impl RendererExtension<'cb, W>
where
    W: TextWrite + 'cb,
{
    RendererExtensionFn::new(move |r: &mut Renderer<'cb, W>| {
        r.add_node_renderer(IconsHtmlRenderer::new, options);
    })
}
// }}} Extension
