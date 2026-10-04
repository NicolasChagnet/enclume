use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::{parsers::MarkdownParserOptions, path::AbsPath};

mod build;
mod collection;
mod content;
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
        #[arg(short, long, default_value = "src")]
        base: PathBuf,
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

fn main() -> Result<()> {
    initialize_logging()?;

    let args = Args::parse();
    match args.command {
        Command::Build {
            base,
            out,
            markdown_args,
        } => {
            let base: AbsPath = base.try_into()?;
            let out: AbsPath = out.try_into()?;

            build::build(base, out, markdown_args.into())?;
        }
    }
    Ok(())
}

fn initialize_logging() -> anyhow::Result<()> {
    env_logger::try_init()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsers::{ContentParser, MarkdownParser};

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
        let (_, body) = MarkdownParser::new(options)
            .parse("Hello :smile:\n")
            .unwrap();
        assert!(body.inner().contains(":smile:"));
    }
}
