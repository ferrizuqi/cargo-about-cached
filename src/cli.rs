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

impl Cli {
    pub fn parse_env() -> Self {
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

        Self::parse_from(std::iter::once(executable).chain(args))
    }
}
