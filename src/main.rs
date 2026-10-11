use clap::{Parser, Subcommand};
use snafu::prelude::*;
use std::path::{Path, PathBuf};

use crate::{
    build::{BuildMode, Builder, SiteBuilder},
    output::OutputLayout,
    parsers::MarkdownParserOptions,
    path::Roots,
};

mod build;
mod content;
mod dev;
mod glob;
mod init;
mod output;
mod pages;
mod parsers;
mod path;
mod templates;

#[derive(Debug, Clone, Parser)]
#[command(about = "A lightweight static site generator.", long_about = None, version)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Clone, Subcommand)]
enum Command {
    Build {
        #[command(flatten)]
        options: BuildOptions,
    },
    Dev {
        #[command(flatten)]
        options: BuildOptions,
    },
    Init,
}

#[derive(Debug, Clone, Default, clap::Args)]
struct BuildOptions {
    /// Input path
    #[arg(short, long, default_value = "src")]
    base: PathBuf,
    /// Output path
    #[arg(short, long, default_value = "dist")]
    out: PathBuf,
    /// Generate pages next to their source (about.md -> about.html)
    #[arg(long)]
    no_clean_urls: bool,
    /// Add a content hash to the name of css and js assets
    #[arg(long)]
    hash_assets: bool,
    /// Markdown options
    #[command(flatten)]
    markdown_args: MarkdownParserCliOptions,
}

#[derive(Debug, Clone, Default, clap::Args)]
struct MarkdownParserCliOptions {
    /// Do not add automatic id attributes to headings
    #[arg(long = "md-no-heading-ids")]
    no_heading_ids: bool,
    /// Omit raw HTML and unsafe links from the rendered Markdown
    #[arg(long = "md-no-raw-html")]
    no_raw_html: bool,
    /// Do not parse footnotes
    #[arg(long = "md-disable-footnote")]
    disable_footnotes: bool,
    /// Do not parse emoji shortcodes
    #[arg(long = "md-disable-emojis")]
    disable_emojis: bool,
    /// Do not parse diagram blocks
    #[arg(long = "md-disable-diagrams")]
    disable_diagrams: bool,
    /// Do not parse fenced div blocks
    #[arg(long = "md-disable-fenced-divs")]
    disable_fenced_divs: bool,
    /// Do not parse math formulas
    #[arg(long = "md-disable-math")]
    disable_math: bool,
    /// Disable code blocks parsing
    #[arg(long = "md-disable-code")]
    disable_code: bool,
    /// Code light theme (e.g. 'github_light')
    #[arg(long = "code-light-theme")]
    code_light_theme: Option<String>,
    /// Code dark theme (e.g. 'catppuccin_macchiato')
    #[arg(long = "code-dark-theme")]
    code_dark_theme: Option<String>,
    /// Disable line numbers in code blocks
    #[arg(long = "code-line-numbers")]
    disable_code_line_numbers: bool,
}

impl From<&BuildOptions> for OutputLayout {
    fn from(options: &BuildOptions) -> Self {
        Self::new(!options.no_clean_urls, options.hash_assets)
    }
}

impl From<MarkdownParserCliOptions> for MarkdownParserOptions {
    fn from(value: MarkdownParserCliOptions) -> Self {
        let mut options = Self::default()
            .with_auto_heading_ids(!value.no_heading_ids)
            .with_raw_html(!value.no_raw_html)
            .with_footnotes(!value.disable_footnotes)
            .with_emojis(!value.disable_emojis)
            .with_diagrams(!value.disable_diagrams)
            .with_fenced_divs(!value.disable_fenced_divs)
            .with_math(!value.disable_math)
            .with_syntax_highlighting(!value.disable_code)
            .with_code_line_numbers(!value.disable_code_line_numbers);

        if let Some(light_theme) = value.code_light_theme {
            options = options.with_code_light_theme(light_theme);
        }
        if let Some(dark_theme) = value.code_dark_theme {
            options = options.with_code_dark_theme(dark_theme);
        }
        options
    }
}

/// The `report` attribute prints the full cause chain when the command fails
#[snafu::report]
#[tokio::main]
async fn main() -> Result<(), snafu::Whatever> {
    initialize_logging()?;

    let args = Args::parse();
    match args.command {
        Command::Build { options } => {
            let roots = Roots::new(options.base.clone(), options.out.clone())?;
            let layout = OutputLayout::from(&options);
            let builder =
                SiteBuilder::new(roots, layout, options.markdown_args.into(), BuildMode::Prod);
            builder.build()?;
        }
        Command::Dev { options } => {
            let roots = Roots::new(options.base.clone(), options.out.clone())?;
            let layout = OutputLayout::from(&options);
            let builder = SiteBuilder::new(
                roots.clone(),
                layout,
                options.markdown_args.clone().into(),
                BuildMode::Dev,
            );
            dev::serve_and_watch(roots, builder).await?;
        }
        Command::Init => {
            init::project(Path::new("src"))?;
        }
    }
    Ok(())
}

fn initialize_logging() -> Result<(), snafu::Whatever> {
    use std::io::Write;
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format(|buf, record| {
            writeln!(
                buf,
                "{}[{}]{} {}",
                buf.default_level_style(record.level()),
                record.level(),
                buf.default_level_style(record.level()).render_reset(),
                record.args()
            )
        })
        .try_init()
        .whatever_context("Failed to initialize logging...")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsers::{ContentParser, MarkdownParser};
    use crate::path::SitePath;

    #[test]
    fn cli_flags_map_to_the_output_layout() {
        let args = Args::parse_from(["enclume", "build"]);
        let Command::Build { options } = args.command else {
            panic!("expected a build command");
        };
        // Clean URLs are on, fingerprinting is off
        assert_eq!(OutputLayout::from(&options), OutputLayout::new(true, false));

        let args = Args::parse_from(["enclume", "build", "--no-clean-urls", "--hash-assets"]);
        let Command::Build { options } = args.command else {
            panic!("expected a build command");
        };
        assert_eq!(OutputLayout::from(&options), OutputLayout::new(false, true));
    }

    #[test]
    fn cli_flags_map_to_markdown_options() {
        let options: MarkdownParserOptions = MarkdownParserCliOptions::default().into();
        assert!(options.parser_options().auto_heading_ids);
        assert!(options.renderer_options().allows_unsafe);

        let options: MarkdownParserOptions = MarkdownParserCliOptions {
            no_heading_ids: true,
            no_raw_html: true,
            ..Default::default()
        }
        .into();
        assert!(!options.parser_options().auto_heading_ids);
        assert!(!options.renderer_options().allows_unsafe);

        let options: MarkdownParserOptions = MarkdownParserCliOptions {
            disable_emojis: true,
            ..Default::default()
        }
        .into();
        let path = SitePath::try_from(PathBuf::from("blog/post.md")).unwrap();
        let metadata = MarkdownParser::new(options)
            .parse(
                "---\ntemplate: post.html\nvars: {}\n---\nHello :smile:\n",
                &path,
            )
            .unwrap()
            .expect("expected frontmatter");
        let value = metadata.as_value().unwrap();
        let content = value["__content__"].as_str().unwrap();
        assert!(content.contains(":smile:"));
    }
}
