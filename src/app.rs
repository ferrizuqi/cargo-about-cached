use std::time::Instant;

use anyhow::Result;

use crate::{
    cache::{CacheLayout, LicenseCache, LockPackageIndex, MetadataCache},
    cli::Cli,
    gather::LicenseResolver,
    generate::OutputGenerator,
    workspace::Workspace,
};

#[derive(Debug)]
pub struct App {
    cli: Cli,
}

impl App {
    pub fn new(cli: Cli) -> Self {
        Self { cli }
    }

    pub fn run(self) -> Result<()> {
        let Cli {
            template,
            output,
            manifest_path,
        } = self.cli;

        let workspace = Workspace::open(manifest_path)?;
        println!("manifest: {}", workspace.manifest_path());

        match workspace.config_path() {
            Some(path) => {
                println!("config: {path}")
            }
            None => {
                println!("config: default")
            }
        }

        let metadata_started = Instant::now();

        let probe = workspace.probe()?;

        let cache_layout = CacheLayout::new(probe.target_directory());
        let metadata_cache = MetadataCache::new(cache_layout.metadata_store());

        let metadata_resolution = metadata_cache.resolve(&workspace, &probe)?;
        let metadata_source = metadata_resolution.source();

        let metadata = metadata_resolution.into_metadata();
        let krates = workspace.build_graph(&metadata)?;

        println!(
            "metadata: {} crates, {} ({:.2?})",
            krates.len(),
            metadata_source,
            metadata_started.elapsed(),
        );
        println!("metadata cache: {}", metadata_cache.directory(),);

        let package_index = LockPackageIndex::new(probe.lockfile())?;

        let license_cache_path = cache_layout.license_store();
        println!("license cache: {license_cache_path}");

        let mut license_cache = LicenseCache::load(license_cache_path.as_std_path())?;
        println!("cached entries before: {}", license_cache.len(),);

        let gather_started = Instant::now();

        let result = LicenseResolver::new(
            &krates,
            &metadata,
            &package_index,
            &mut license_cache,
            workspace.config(),
            workspace.config_fingerprint(),
        )
        .resolve()?;

        println!("license cache: {} hit, {} miss", result.hits, result.misses,);
        println!("gather elapsed: {:.2?}", gather_started.elapsed(),);

        license_cache.save(license_cache_path.as_std_path())?;

        let generator = OutputGenerator::new(template, output);
        let generate_started = Instant::now();
        generator.generate(&result.licenses, workspace.into_config())?;

        println!(
            "generated: {} ({:.2?})",
            generator.output_path().display(),
            generate_started.elapsed(),
        );

        Ok(())
    }
}
