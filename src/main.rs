use clap::{Parser, Subcommand};
use snafu::prelude::*;
use std::path::PathBuf;

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
}

impl From<&BuildOptions> for OutputLayout {
    fn from(options: &BuildOptions) -> Self {
        Self::new(!options.no_clean_urls, options.hash_assets)
    }
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
            init_project()?;
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

/// Initializes a project structure at the current directory
///
/// Creates the following directories under `src/`
/// ```text
/// src/
///    templates/
///       layout.html
///    root/
///       index.md
///       404.md
/// ```
fn init_project() -> Result<(), snafu::Whatever> {
    std::fs::create_dir_all("./src/templates")
        .whatever_context("Couldn't create `templates` directory")?;
    std::fs::create_dir_all("./src/root").whatever_context("Couldn't create `root` directory")?;

    std::fs::write(
        "./src/templates/layout.html",
        r#"
<!DOCTYPE html>
<html lang="en">
    <head>
        <title>{{ vars.title }}</title>
    </head>
    <body>
        {{__content__}}
    </body>
</html>"#,
    )
    .whatever_context("Couldn't write layout.html template")?;

    std::fs::write(
        "./src/root/index.md",
        r#"---
template: layout.html
vars:
    title: Home
---
Hello, World!
"#,
    )
    .whatever_context("Couldn't write index.md template")?;

    std::fs::write(
        "./src/root/404.md",
        r#"---
template: layout.html
vars:
    title: 404 Not Found
---
Page not found
"#,
    )
    .whatever_context("Couldn't write 404.md template")?;
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
