use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::path::AbsPath;

mod build;
mod collection;
mod content;
mod glob;
mod parsers;
mod path;
mod templates;

#[derive(Debug, Clone, Parser)]
#[command(about = "A lightweight static site generator.", long_about = None)]
pub struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    Build {
        #[arg(short, long, default_value = "src")]
        base: PathBuf,
        #[arg(short, long, default_value = "dist")]
        out: PathBuf,
    },
}

fn main() -> Result<()> {
    initialize_logging()?;

    let args = Args::parse();
    match args.command {
        Command::Build { base, out } => {
            let base: AbsPath = base.try_into()?;
            let out: AbsPath = out.try_into()?;
            build::build(base, out)?;
        }
    }
    Ok(())
}

fn initialize_logging() -> anyhow::Result<()> {
    env_logger::try_init()?;
    Ok(())
}
