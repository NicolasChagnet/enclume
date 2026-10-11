//! Highlight the code blocks of a rushdown document with lumis.
//!
//! [lumis](https://lumis.sh) highlights code with the grammars of
//! [tree-sitter](https://tree-sitter.github.io/tree-sitter/) and the
//! colorschemes of Neovim, and writes the block as HTML itself. This crate
//! hands it every code block of a rushdown document, with the language of the
//! fence and the options written after it:
//!
//! ```
//! let theme: rushdown_lumis::Theme = "catppuccin_mocha".parse().unwrap();
//!
//! let parser = rushdown::new_markdown_to_html(
//!     Default::default(),
//!     Default::default(),
//!     rushdown::parser::empty_parser_extension(),
//!     rushdown_lumis::lumis_html_renderer_extension(rushdown_lumis::Options::new(theme)),
//! );
//!
//! let mut html = String::new();
//! parser(&mut html, "```rust\nlet answer = 42;\n```").unwrap();
//! assert!(html.contains(r#"<code class="language-rust""#));
//! assert!(html.contains(r#"<span style="color: #cba6f7;">let</span>"#));
//! ```
//!
//! # The stylesheet
//!
//! None is needed. Every colour of a block is written inline — the block takes
//! its own colours from the theme, each line is written as a `<span>` of its
//! own, and a marked line and a line number are given their colours by the
//! theme too. A page which wants the small niceties lumis stylesheets add, or
//! a stylesheet of its own for them, can ask `lumis themes build-css <theme>`
//! for the classes of the theme.
//!
//! # Themes
//!
//! A theme is one of the colorschemes lumis ships — github_dark, dracula,
//! gruvbox_dark, catppuccin_mocha, nord, and 500 others — read by name:
//!
//! ```
//! let theme: rushdown_lumis::Theme = "github_dark".parse().unwrap();
//! let options = rushdown_lumis::Options::new(theme).with_line_numbers(true);
//! ```
//!
//! Giving [`Options`] a second theme of the other appearance sets a block for
//! light and dark at once, with the `light-dark()` function of CSS. The page
//! then declares which the browser may use, and reads the rest from the
//! reader's own preference:
//!
//! ```css
//! :root { color-scheme: light dark; }
//! ```
//!
//! ```
//! let (light, dark): (rushdown_lumis::Theme, rushdown_lumis::Theme) =
//!     ("catppuccin_latte".parse().unwrap(), "catppuccin_mocha".parse().unwrap());
//! let options = rushdown_lumis::Options::new(light).with_theme(dark);
//! ```
//!
//! # What a code block supports
//!
//! The language of a fence is read as the name or the alias of a lumis
//! language — `rust`, `js`, `sh`, `yml`, `text` — and a language which is not
//! known is set plain. The rest of the info string holds the options, written
//! as a key and a value separated by `=`, or as a key on its own which reads
//! as `true`:
//!
//! ```text
//! rust line_numbers highlight_lines=3,7-9 title="src/main.rs"
//! ```
//!
//! - `line_numbers` adds the gutter of line numbers, which count from one;
//! - `highlight_lines=3,7-9` marks lines, which the theme draws in the colour
//!   it gives to `highlighted`, and which carry the class `l-highlighted`
//!   lumis gives them in its own markup, for a page which styles them further;
//! - `title="src/main.rs"` writes the title bar of the block, a
//!   `<figure class="lumis-figure">` which holds the title in a
//!   `<figcaption class="lumis-title">` and then the block itself. The page
//!   styles the bar, since no theme holds its colours.
//!
//! [`Options::with_line_numbers`] and [`Options::with_italic`] set a site-wide
//! default for a page whose blocks mostly want the same, and
//! [`Options::with_highlights`] names the scope every token was highlighted as
//! — `keyword`, `comment`, `function.call` — in the `data-highlight` attribute
//! of its `<span>`, for a page which styles those names itself. A block has no
//! frame: a site which wants one wraps the block in its own markup.
//!
//! # Languages
//!
//! A site highlights the languages its build turns on, and lumis compiles one
//! parser per language, so this crate turns on a set which covers a blog —
//! Rust, Shell, C, C++, Go, JavaScript, TypeScript, HTML, CSS, Python, SQL,
//! JSON, YAML, TOML, Markdown and diff. Four features add more: `web`,
//! `backend`, `config-files`, and `all-languages`. A site which needs one
//! language outside the default set depends on lumis itself with the feature
//! for it, which cargo joins to the ones this crate asks for:
//!
//! ```toml
//! [dependencies]
//! lumis = { version = "0.17", features = ["lang-nix"] }
//! ```

mod meta;
mod render;

use rushdown::renderer::{
    RendererOptions, TextWrite,
    html::{Renderer, RendererExtension, RendererExtensionFn},
};

pub use lumis::languages::Language;
pub use lumis::themes::{Appearance, Theme, get as get_theme};

/// Options for the lumis HTML renderer
///
/// The themes are given in the order the page styles itself: one theme is the
/// theme of every block, and two themes of opposite [`Appearance`] set the
/// block for both at once, through the `light-dark()` function of CSS. A
/// second theme which is not of the other appearance is left out, and the
/// first theme styles every block.
#[derive(Clone, Debug)]
pub struct Options {
    themes: Vec<Theme>,
    line_numbers: bool,
    italic: bool,
    highlights: bool,
}

impl Options {
    /// Creates options which highlight every code block with this theme
    pub fn new(theme: Theme) -> Self {
        Self {
            themes: vec![theme],
            line_numbers: false,
            italic: false,
            highlights: false,
        }
    }

    /// Adds the theme of the other appearance, for a page read in both
    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.themes.push(theme);
        self
    }

    /// Shows the line numbers of code blocks which do not say otherwise
    pub fn with_line_numbers(mut self, line_numbers: bool) -> Self {
        self.line_numbers = line_numbers;
        self
    }

    /// Writes the italic of the theme, which lumis leaves out by default
    ///
    /// Most colorschemes set comments in italic, and a few go further.
    pub fn with_italic(mut self, italic: bool) -> Self {
        self.italic = italic;
        self
    }

    /// Names the highlight scope of every token, beside the colour of the
    /// theme
    ///
    /// Every token is a `<span>` which holds the name of the scope it was
    /// highlighted as, such as `keyword`, `comment`, or `function.call`, in
    /// its `data-highlight` attribute. A page which styles those names itself
    /// asks for them; a page which does not pays for them in every block.
    pub fn with_highlights(mut self, highlights: bool) -> Self {
        self.highlights = highlights;
        self
    }

    /// The theme of every block, and the theme of the other appearance
    pub(crate) fn themes(&self) -> &[Theme] {
        &self.themes
    }

    /// Whether blocks show their line numbers unless they say otherwise
    pub(crate) fn line_numbers(&self) -> bool {
        self.line_numbers
    }

    /// Whether the italic of a theme is written
    pub(crate) fn italic(&self) -> bool {
        self.italic
    }

    /// Whether the scope of every token is written beside its colour
    pub(crate) fn highlights(&self) -> bool {
        self.highlights
    }
}

impl RendererOptions for Options {}

/// Returns an HTML renderer extension writing code blocks with lumis
pub fn lumis_html_renderer_extension<'cb, W>(options: Options) -> impl RendererExtension<'cb, W>
where
    W: TextWrite + 'cb,
{
    RendererExtensionFn::new(move |renderer: &mut Renderer<'cb, W>| {
        renderer.add_node_renderer(render::LumisHtmlRenderer::new, options);
    })
}
