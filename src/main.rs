use clap::{Parser, Subcommand};
use snafu::prelude::*;
use std::path::PathBuf;

use crate::{
    build::{Builder, SiteBuilder},
    parsers::MarkdownParserOptions,
    path::Roots,
};

mod build;
mod content;
mod dev;
mod glob;
mod parsers;
mod path;
mod templates;

#[derive(Debug, Clone, Parser)]
#[command(about = "A lightweight static site generator.", long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Clone, Subcommand)]
enum Command {
    Build {
        /// Input path
        #[arg(short, long, default_value = "src")]
        base: PathBuf,
        /// Output path
        #[arg(short, long, default_value = "dist")]
        out: PathBuf,
        #[command(flatten)]
        markdown_args: MarkdownParserCliOptions,
    },
    Dev {
        /// Input path
        #[arg(short, long, default_value = "src")]
        base: PathBuf,
        /// Output path
        #[arg(short, long, default_value = "dist")]
        out: PathBuf,
        #[command(flatten)]
        markdown_args: MarkdownParserCliOptions,
    },
}

#[derive(Debug, Clone, Default, clap::Args)]
struct MarkdownParserCliOptions {
    /// Do not add automatic id attributes to headings
    #[arg(long)]
    no_heading_ids: bool,
    /// Omit raw HTML and unsafe links from the rendered Markdown
    #[arg(long)]
    no_raw_html: bool,
    /// Do not parse footnotes
    #[arg(long)]
    disable_footnotes: bool,
    /// Do not parse emoji shortcodes
    #[arg(long)]
    disable_emojis: bool,
    /// Do not parse diagram blocks
    #[arg(long)]
    disable_diagrams: bool,
    /// Do not parse fenced div blocks
    #[arg(long)]
    disable_fenced_divs: bool,
}

impl From<MarkdownParserCliOptions> for MarkdownParserOptions {
    fn from(value: MarkdownParserCliOptions) -> Self {
        Self::default()
            .with_auto_heading_ids(!value.no_heading_ids)
            .with_raw_html(!value.no_raw_html)
            .with_footnotes(!value.disable_footnotes)
            .with_emojis(!value.disable_emojis)
            .with_diagrams(!value.disable_diagrams)
            .with_fenced_divs(!value.disable_fenced_divs)
    }
}

/// The `report` attribute prints the full cause chain when the command fails
#[snafu::report]
#[tokio::main]
async fn main() -> Result<(), snafu::Whatever> {
    initialize_logging()?;

    let args = Args::parse();
    match args.command {
        Command::Build {
            base,
            out,
            markdown_args,
        } => {
            let roots = Roots::new(base, out)?;
            let builder = SiteBuilder::new(roots, markdown_args.into());
            builder.build()?;
        }
        Command::Dev {
            base,
            out,
            markdown_args,
        } => {
            let roots = Roots::new(base, out)?;
            let builder = SiteBuilder::new(roots.clone(), markdown_args.into());
            dev::serve_and_watch(roots, builder).await?;
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
    use crate::parsers::{ContentParser, MarkdownParser, VAR_CONTENT};

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
        let metadata = MarkdownParser::new(options)
            .parse("---\ntemplate: post.html\n---\nHello :smile:\n")
            .unwrap()
            .expect("expected frontmatter");
        let content = metadata
            .variables()
            .get(VAR_CONTENT)
            .and_then(|content| content.as_str())
            .unwrap();
        assert!(content.contains(":smile:"));
    }
}
