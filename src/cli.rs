use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
};

use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "cargo-about-cached")]
pub struct Cli {
    /// Handlebars template
    pub template: PathBuf,

    /// Output file
    #[arg(short, long)]
    pub output: PathBuf,

    /// Path to Cargo.toml
    #[arg(long)]
    pub manifest_path: Option<PathBuf>,
}

pub fn parse_cli() -> Cli {
    let mut args = std::env::args_os();

    let executable = args
        .next()
        .unwrap_or_else(|| OsString::from("cargo-about-cached"));

    let mut args = args.collect::<Vec<_>>();

    if args
        .first()
        .is_some_and(|arg| arg == OsStr::new("about-cached"))
    {
        args.remove(0);
    }

    Cli::parse_from(std::iter::once(executable).chain(args))
}
