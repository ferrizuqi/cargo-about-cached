mod cached;
mod cli;
mod gather;
mod generate;

use std::{path::PathBuf, time::Instant};

use anyhow::{Context, Result, anyhow};
use cargo_about::{Krates, licenses::config::Config};
use cargo_lock::Lockfile;
use krates::{Builder, DepKind, Scope, Utf8Path, Utf8PathBuf};
use toml_span::Deserialize as _;

use cached::{CacheStore, LockPackageIndex};
use gather::gather_cached;
use generate::generate_output;

use crate::{cached::CachedConfigFingerprint, cli::parse_cli};

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = parse_cli();

    let manifest_path = manifest_path(cli.manifest_path)?;
    anyhow::ensure!(
        manifest_path.exists(),
        "Cargo manifest does not exist: {manifest_path}"
    );
    println!("manifest: {manifest_path}");

    let (config, config_fingerprint) = load_config(&manifest_path)?;
    let metadata_started = Instant::now();
    let (metadata, krates) = load_krates(&manifest_path, &config)?;

    println!(
        "metadata: {} crates ({:.2?})",
        krates.len(),
        metadata_started.elapsed(),
    );

    let lockfile_path = metadata.workspace_root.join("Cargo.lock");

    let lockfile = Lockfile::load(&lockfile_path)
        .with_context(|| format!("failed to load Cargo.lock: {lockfile_path}"))?;

    let package_index = LockPackageIndex::new(&lockfile)?;

    let cache_path = metadata
        .target_directory
        .join(".cargo-about-cached")
        .join("cache.json");
    println!("cache: {cache_path}");

    let mut cache = CacheStore::load(&cache_path, &config_fingerprint)?;
    println!("cached entries before: {}", cache.len());

    let gather_started = Instant::now();

    let result = gather_cached(&krates, &metadata, &package_index, &mut cache, &config)?;

    println!("cache: {} hit, {} miss", result.hits, result.misses,);
    println!("gather elapsed: {:.2?}", gather_started.elapsed(),);

    cache.save(&cache_path)?;

    let generate_started = Instant::now();

    generate_output(&result.licenses, config, &cli.template, &cli.output)?;

    println!(
        "generated: {} ({:.2?})",
        cli.output.display(),
        generate_started.elapsed(),
    );

    Ok(())
}

fn manifest_path(manifest_path: Option<PathBuf>) -> Result<Utf8PathBuf> {
    let path = match manifest_path {
        Some(path) => path,

        None => std::env::current_dir()
            .context("failed to determine current directory")?
            .join("Cargo.toml"),
    };

    Utf8PathBuf::from_path_buf(path)
        .map_err(|path| anyhow!("manifest path is not valid UTF-8: {}", path.display()))
}

fn load_config(manifest_path: &Utf8Path) -> Result<(Config, CachedConfigFingerprint)> {
    let mut parent = manifest_path.parent();

    while let Some(directory) = parent {
        let path = directory.join("about.toml");

        if path.exists() {
            println!("config: {path}");

            let contents = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read cargo-about config: {path}"))?;

            let fingerprint = CachedConfigFingerprint::from_contents(Some(&contents));

            let config = deserialize_config(&path, &contents)?;

            return Ok((config, fingerprint));
        }

        parent = directory.parent();
    }

    println!("config: default");

    Ok((
        Config::default(),
        CachedConfigFingerprint::from_contents(None),
    ))
}

fn deserialize_config(path: &Utf8Path, contents: &str) -> Result<Config> {
    let mut value = toml_span::parse(contents).map_err(|error| {
        anyhow!(
            "failed to parse cargo-about config \
                 '{path}': {error}"
        )
    })?;

    Config::deserialize(&mut value).map_err(|error| {
        let errors = error
            .errors
            .into_iter()
            .map(|error| error.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        anyhow!(
            "failed to deserialize cargo-about \
             config '{path}':\n{errors}"
        )
    })
}

fn load_krates(
    manifest_path: &Utf8Path,
    config: &Config,
) -> Result<(krates::cm::Metadata, Krates)> {
    let mut command = krates::Cmd::new();
    command.manifest_path(manifest_path);

    let metadata = krates::cm::MetadataCommand::from(command)
        .exec()
        .context("cargo metadata failed")?;

    let mut builder = Builder::new();

    if config.ignore_build_dependencies {
        builder.ignore_kind(DepKind::Build, Scope::All);
    }

    if config.ignore_dev_dependencies {
        builder.ignore_kind(DepKind::Dev, Scope::All);
    }

    if config.ignore_transitive_dependencies {
        builder.ignore_kind(DepKind::Normal, Scope::NonWorkspace);
        builder.ignore_kind(DepKind::Dev, Scope::NonWorkspace);
        builder.ignore_kind(DepKind::Build, Scope::NonWorkspace);
    }

    if !config.targets.is_empty() {
        builder.include_targets(
            config
                .targets
                .iter()
                .map(|target| (target.as_str(), vec![])),
        );
    }

    let krates: Krates = builder
        .build_with_metadata(metadata.clone(), |_: krates::cm::Package| {})
        .context("failed to build crate graph")?;

    Ok((metadata, krates))
}
