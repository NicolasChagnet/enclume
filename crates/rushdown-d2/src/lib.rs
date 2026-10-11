//! Draw the `d2` code blocks of a rushdown document as inline SVG.
//!
//! [D2](https://d2lang.com) draws a diagram from a small language of its own.
//! The `d2` command does the drawing; this crate hands it every block fenced
//! with `d2`, and writes what it draws straight into the page, which needs no
//! script and no stylesheet:
//!
//! ```no_run
//! use rushdown::new_markdown_to_html;
//!
//! let parser = new_markdown_to_html(
//!     Default::default(),
//!     Default::default(),
//!     rushdown_d2::d2_parser_extension(),
//!     rushdown_d2::d2_html_renderer_extension(rushdown_d2::Options::default()),
//! );
//!
//! let mut html = String::new();
//! parser(&mut html, "```d2\na -> b\n```").unwrap();
//! assert!(html.contains("<svg"));
//! ```
//!
//! The diagram is written as `<figure class="d2"><div>…</div></figure>`, which
//! a page styles like any other figure.
//!
//! # Light and dark
//!
//! The SVG holds both appearances at once: every colour it draws is the two
//! sides of the `light-dark()` function of CSS, which the `color-scheme` of
//! the page picks. A page which follows the preference of its reader declares
//! it, as it does for the code blocks of a page:
//!
//! ```css
//! :root { color-scheme: light dark; }
//! ```
//!
//! The diagram then wears the same appearance as the rest of the page, and a
//! reader who chooses one — a switcher which sets `color-scheme` — is followed
//! by both.
//!
//! [`Options::with_theme`] and [`Options::with_dark_theme`] take the ids of
//! the two themes, which `d2 themes` prints.
//!
//! # The command
//!
//! `d2` is looked up on the `PATH` of the build, and [`Options::with_command`]
//! names another binary. A diagram which the command refuses to draw is an
//! error which carries what `d2` said, so that a build stops at the diagram
//! instead of writing a page without it.

use std::any::TypeId;
use std::fmt;
use std::io::Write as _;
use std::marker::PhantomData;
use std::ops::Range;
use std::process::{Command, Stdio};

use rushdown::{
    Result, as_extension_data, as_kind_data, as_type_data, as_type_data_mut,
    ast::{Arena, KindData, NodeKind, NodeRef, NodeType, PrettyPrint, WalkStatus, pp_indent, walk},
    matches_kind,
    parser::{
        AnyAstTransformer, AstTransformer, Context, NoParserOptions, Parser, ParserExtension,
        ParserExtensionFn,
    },
    renderer::{
        self, BoxRenderNode, NodeRenderer, NodeRendererRegistry, RenderNode, RendererOptions,
        TextWrite,
        html::{self, Renderer, RendererExtension, RendererExtensionFn},
    },
    text::{self, BasicReader, Reader as _},
};

/// A d2 diagram in the AST, which holds the source of its code block.
#[derive(Debug, Default)]
pub struct D2Diagram {
    value: text::Lines,
}

impl D2Diagram {
    /// The source of the diagram, as the lines of its code block.
    pub fn value(&self) -> &text::Lines {
        &self.value
    }

    /// Sets the source of the diagram.
    pub fn set_value(&mut self, value: impl Into<text::Lines>) {
        self.value = value.into();
    }
}

impl NodeKind for D2Diagram {
    fn typ(&self) -> NodeType {
        NodeType::LeafBlock
    }

    fn kind_name(&self) -> &'static str {
        "D2Diagram"
    }
}

impl PrettyPrint for D2Diagram {
    fn pretty_print(&self, w: &mut dyn fmt::Write, source: &str, level: usize) -> fmt::Result {
        writeln!(w, "{}Value: [", pp_indent(level))?;
        for line in self.value.iter(source) {
            write!(w, "{}{}", pp_indent(level + 1), line)?;
        }
        writeln!(w)?;
        write!(w, "{}]", pp_indent(level))
    }
}

impl From<D2Diagram> for KindData {
    fn from(e: D2Diagram) -> Self {
        KindData::Extension(Box::new(e))
    }
}

/// Replaces the code blocks fenced with `d2` with [`D2Diagram`] nodes
#[derive(Debug, Default)]
struct D2AstTransformer;

impl AstTransformer for D2AstTransformer {
    fn transform(
        &self,
        arena: &mut Arena,
        document: NodeRef,
        reader: &mut BasicReader,
        _context: &mut Context,
    ) {
        let mut targets = Vec::new();
        walk(arena, document, &mut |arena: &Arena,
                                    node_ref: NodeRef,
                                    entering: bool|
         -> Result<WalkStatus> {
            if entering && matches_kind!(arena[node_ref], CodeBlock) {
                let language =
                    as_kind_data!(arena[node_ref], CodeBlock).language_str(reader.source());
                if language == Some("d2") {
                    targets.push(node_ref);
                }
            }
            Ok(WalkStatus::Continue)
        })
        .ok();

        for code_ref in targets {
            let Some(parent) = arena[code_ref].parent() else {
                continue;
            };
            let source = as_kind_data!(arena[code_ref], CodeBlock).value().clone();
            let position = arena[code_ref].pos();
            let blank_previous_line =
                as_type_data!(arena, code_ref, Block).has_blank_previous_line();

            let mut diagram = D2Diagram::default();
            diagram.set_value(source);
            let diagram_ref = arena.new_node(diagram);
            if let Some(position) = position {
                arena[diagram_ref].set_pos(position);
            }
            as_type_data_mut!(arena, diagram_ref, Block)
                .set_blank_previous_line(blank_previous_line);
            parent.replace_child(arena, code_ref, diagram_ref);
        }
    }
}

impl From<D2AstTransformer> for AnyAstTransformer {
    fn from(transformer: D2AstTransformer) -> Self {
        AnyAstTransformer::Extension(Box::new(transformer))
    }
}

/// Returns a parser extension that reads the code blocks fenced with `d2`
pub fn d2_parser_extension() -> impl ParserExtension {
    ParserExtensionFn::new(|p: &mut Parser| {
        p.add_ast_transformer(|| D2AstTransformer, NoParserOptions, 100);
    })
}

/// Options for the d2 HTML renderer.
#[derive(Debug, Clone)]
pub struct Options {
    command: String,
    theme: u16,
    dark_theme: u16,
}

impl Options {
    /// The id of the theme of a light page, `Neutral Default` in `d2 themes`.
    pub const DEFAULT_THEME: u16 = 0;

    /// The id of the theme of a dark page, `Dark Mauve` in `d2 themes`.
    pub const DEFAULT_DARK_THEME: u16 = 200;

    /// Names the command to run, which is `d2` itself by default
    pub fn with_command(mut self, command: impl Into<String>) -> Self {
        self.command = command.into();
        self
    }

    /// Sets the theme of a page read in a light appearance
    pub fn with_theme(mut self, theme: u16) -> Self {
        self.theme = theme;
        self
    }

    /// Sets the theme of a page read in a dark appearance
    pub fn with_dark_theme(mut self, dark_theme: u16) -> Self {
        self.dark_theme = dark_theme;
        self
    }

    /// Draws the diagram with the command, and returns what it wrote
    fn draw(&self, source: &str) -> Result<String> {
        let output = run(self, source).map_err(|e| {
            rushdown::Error::io(
                format!("d2 could not draw the diagram: {e}"),
                // The failure keeps the error of the command as its source
                Some(Box::new(e)),
            )
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let message = if stderr.trim().is_empty() {
                format!("{} exited with {}", self.command, output.status)
            } else {
                stderr.trim().to_string()
            };
            return Err(rushdown::Error::invalid_node_operation(format!(
                "d2 could not draw the diagram: {message}"
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

impl Default for Options {
    fn default() -> Self {
        Self {
            command: "d2".to_string(),
            theme: Self::DEFAULT_THEME,
            dark_theme: Self::DEFAULT_DARK_THEME,
        }
    }
}

impl RendererOptions for Options {}

/// Draws the diagram of every [`D2Diagram`] node of a page
struct D2HtmlRenderer<W: TextWrite> {
    _phantom: PhantomData<W>,
    options: Options,
    writer: html::Writer,
}

impl<W: TextWrite> D2HtmlRenderer<W> {
    fn new(html_options: html::Options, options: Options) -> Self {
        Self {
            _phantom: PhantomData,
            options,
            writer: html::Writer::with_options(html_options),
        }
    }
}

impl<W: TextWrite> RenderNode<W> for D2HtmlRenderer<W> {
    fn render_node<'a>(
        &self,
        w: &mut W,
        source: &'a str,
        arena: &'a Arena,
        node_ref: NodeRef,
        entering: bool,
        _ctx: &mut renderer::Context,
    ) -> Result<WalkStatus> {
        if entering {
            let diagram = as_extension_data!(arena, node_ref, D2Diagram);
            let source = diagram.value().iter(source).collect::<String>();
            self.writer
                .write_html(w, &figure(&self.options.draw(&source)?))?;
        }
        Ok(WalkStatus::Continue)
    }
}

impl<'cb, W> NodeRenderer<'cb, W> for D2HtmlRenderer<W>
where
    W: TextWrite + 'cb,
{
    fn register_node_renderer_fn(self, nrr: &mut impl NodeRendererRegistry<'cb, W>) {
        nrr.register_node_renderer_fn(TypeId::of::<D2Diagram>(), BoxRenderNode::new(self));
    }
}

/// Returns a renderer extension that draws d2 diagrams as inline SVG
pub fn d2_html_renderer_extension<'cb, W>(options: Options) -> impl RendererExtension<'cb, W>
where
    W: TextWrite + 'cb,
{
    RendererExtensionFn::new(move |r: &mut Renderer<'cb, W>| {
        r.add_node_renderer(D2HtmlRenderer::new, options);
    })
}

/// Runs the command on the diagram, with the theme of each appearance
fn run(options: &Options, source: &str) -> std::io::Result<std::process::Output> {
    let mut child = Command::new(&options.command)
        .arg("--no-xml-tag")
        .args(["--theme", &options.theme.to_string()])
        .args(["--dark-theme", &options.dark_theme.to_string()])
        // The source is read from stdin and the SVG written to stdout
        .args(["-", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        // The command reads the whole source before it draws anything, so
        // closing the pipe here cannot deadlock on a large diagram
        stdin.write_all(source.as_bytes())?;
    }

    child.wait_with_output()
}

/// The rule d2 writes before the colours of the dark appearance
const DARK_APPEARANCE: &str = "@media screen and (prefers-color-scheme:dark){";

/// Wraps a diagram in the figure of a d2 diagram, with both appearances on
/// the `color-scheme` of the page
fn figure(svg: &str) -> String {
    format!(
        "<figure class=\"d2\"><div>{}</div></figure>\n",
        on_color_scheme(svg)
    )
}

/// Reads the dark appearance of a diagram into the second side of
/// `light-dark()`.
///
/// d2 writes the dark appearance of a diagram behind `prefers-color-scheme`,
/// which follows the preference of the operating system alone: a page whose
/// reader has chosen an appearance — with `color-scheme` — would keep its
/// choice, and the diagram would not follow it. Merging the two appearances
/// into `light-dark(light, dark)` puts the diagram on the same `color-scheme`
/// as the page around it.
fn on_color_scheme(svg: &str) -> String {
    let Some(start) = svg.find(DARK_APPEARANCE) else {
        return svg.to_owned();
    };
    let Some(end) = block_end(svg, start + DARK_APPEARANCE.len() - 1) else {
        return svg.to_owned();
    };

    let mut sheet = svg[..start].to_owned();
    let mut left = Vec::new();
    for (selector, declarations) in blocks(&svg[start + DARK_APPEARANCE.len()..end - 1]) {
        if !merge_rule(&mut sheet, selector, declarations) {
            left.push((selector, declarations));
        }
    }

    // A rule which found no light rule to merge into keeps the media query,
    // since dropping it would drop the appearance the rule draws
    if !left.is_empty() {
        sheet.push_str(DARK_APPEARANCE);
        for (selector, declarations) in left {
            sheet.push_str(selector);
            sheet.push('{');
            sheet.push_str(declarations);
            sheet.push('}');
        }
        sheet.push('}');
    }

    sheet.push_str(&svg[end..]);
    sheet
}

/// Merges the declarations of one dark rule into the rule of the same
/// selector, and answers whether all of them found a place
fn merge_rule(sheet: &mut String, selector: &str, dark: &str) -> bool {
    let dark = declarations_of(dark);
    // `light-dark()` takes colours, so a rule which the dark appearance writes
    // for something else — the display of a code block, the blend of a stroke
    // — keeps the media query it was written with
    if dark.is_empty() || !dark.iter().all(|(property, _)| carries_a_color(property)) {
        return false;
    }
    let Some(body) = rule_body(sheet, selector) else {
        return false;
    };
    let declarations = sheet[body.start..body.end].to_owned();
    if declarations.contains("data:") {
        // A rule which carries a data URI is left alone: d2 does not write one
        // for a colour of a theme, and splitting one would break it
        return false;
    }

    let mut merged = String::new();
    let mut placed = 0;
    for (property, value) in declarations_of(&declarations) {
        match dark.iter().find(|(dark, _)| *dark == property) {
            Some((_, dark_value)) => {
                placed += 1;
                merged.push_str(&property);
                merged.push_str(":light-dark(");
                merged.push_str(&value);
                merged.push(',');
                merged.push_str(dark_value);
                merged.push_str(");");
            }
            None => {
                merged.push_str(&property);
                merged.push(':');
                merged.push_str(&value);
                merged.push(';');
            }
        }
    }

    if placed != dark.len() {
        // A value the light rule does not carry cannot be merged, and would be
        // dropped by the merge of the others
        return false;
    }

    sheet.replace_range(body, &merged);
    true
}

/// The `property: value` pairs of the declarations of a rule
fn declarations_of(rule: &str) -> Vec<(String, String)> {
    rule.split(';')
        .filter_map(|declaration| {
            let (property, value) = declaration.split_once(':')?;
            let property = property.trim();
            (!property.is_empty()).then(|| (property.to_owned(), value.trim().to_owned()))
        })
        .collect()
}

/// Answers whether a property of a diagram is one which carries a colour
fn carries_a_color(property: &str) -> bool {
    property.starts_with("--")
        || property.ends_with("color")
        || property == "fill"
        || property == "stroke"
}

/// The offsets of the declarations of the rule of a selector, when the
/// stylesheet holds one
fn rule_body(sheet: &str, selector: &str) -> Option<Range<usize>> {
    let mut from = 0;
    while let Some(at) = sheet[from..].find(selector).map(|at| from + at) {
        let after = at + selector.len();
        let rest = &sheet[after..];
        let trimmed = rest.trim_start();
        if trimmed.starts_with('{') {
            let open = after + rest.len() - trimmed.len();
            let end = block_end(sheet, open)?;
            return Some(open + 1..end - 1);
        }
        from = after;
    }
    None
}

/// The `selector{declarations}` blocks of a stylesheet, in the order written
fn blocks(css: &str) -> Vec<(&str, &str)> {
    let mut blocks = Vec::new();
    let mut rest = css;
    while let Some(open) = rest.find('{') {
        let Some(end) = block_end(rest, open) else {
            break;
        };
        blocks.push((rest[..open].trim(), &rest[open + 1..end - 1]));
        rest = &rest[end..];
    }
    blocks
}

/// The offset just past the block which the `{` at `open` starts
fn block_end(css: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in css.as_bytes()[open..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(open + offset + 1);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stylesheet d2 writes for a diagram: a rule for each appearance, and
    /// a rule which the dark appearance writes for the display of a code block
    const SVG: &str = "<svg><style>.d2-x .fill{fill:#fff;}.d2-x .code{display: block}\
        @media screen and (prefers-color-scheme:dark){.d2-x .fill{fill:#000;}\
        .d2-x .code{display: none}}</style></svg>";

    #[test]
    fn puts_the_two_appearances_on_the_color_scheme_of_the_page() {
        let merged = on_color_scheme(SVG);

        assert!(
            merged.contains(".d2-x .fill{fill:light-dark(#fff,#000);}"),
            "one colour holds both appearances: {merged}"
        );
        assert!(
            !merged.contains("fill:#000;"),
            "the dark appearance is not written twice: {merged}"
        );
    }

    #[test]
    fn keeps_the_media_query_of_a_rule_which_draws_no_colour() {
        let merged = on_color_scheme(SVG);

        assert!(
            merged.contains("prefers-color-scheme:dark){.d2-x .code{display: none}}"),
            "`light-dark()` takes colours, so the display of a code block keeps \
             the media query it was written with: {merged}"
        );
    }

    #[test]
    fn wraps_the_diagram_in_the_figure_of_a_d2_diagram() {
        assert_eq!(
            figure("<svg/>"),
            "<figure class=\"d2\"><div><svg/></div></figure>\n"
        );
    }
}
