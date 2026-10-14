use std::path::Path;

use anyhow::Result;

use super::support::{CacheAssertions, CompatibilityContext, FixtureWorkspace};

const OLD_IROH_DEPENDENCY: &str = r#"iroh = "=0.90.0""#;
const NEW_IROH_DEPENDENCY: &str = r#"iroh = "=1.3.0""#;
const NEW_IROH_VERSION: &str = "1.3.0";

pub(crate) struct LargeWorkspaceScenario<'a> {
    context: &'a CompatibilityContext,
}

impl<'a> LargeWorkspaceScenario<'a> {
    pub(crate) fn new(context: &'a CompatibilityContext) -> Self {
        Self { context }
    }

    pub(crate) fn run(&self) -> Result<()> {
        println!("large: creating fixture");
        let fixture = FixtureWorkspace::create(self.context, "large_workspace")?;

        println!("large: generating initial Cargo.lock");
        fixture.generate_lockfile()?;

        /*
         * Phase 1
         *
         * No cache exists.
         */

        println!();
        println!("large: phase 1 - cold cache");

        fixture.cache_state().assert_empty("large cold phase");

        println!("large: generating baseline");
        let initial_baseline = fixture.generate_baseline(self.context, "baseline-initial.txt")?;

        println!("large: running cargo-about-cached");
        let cold = fixture.run_cached(self.context, "cached-cold.txt")?;

        println!("large: baseline {}", initial_baseline);
        println!("large: cached   {}", cold.hash());

        assert_eq!(
            &initial_baseline,
            cold.hash(),
            "large cold-cache output differs from cargo-about",
        );

        let cold_stats = cold.license_cache_stats()?;
        println!(
            "large: license cache {} hit, {} miss",
            cold_stats.hits, cold_stats.misses,
        );

        CacheAssertions::cold(&fixture, &cold)?;

        println!("large: phase 1 passed");

        /*
         * Phase 2
         *
         * Same dependency graph.
         * Everything cacheable should hit.
         */

        println!();
        println!("large: phase 2 - warm cache");

        fixture.cache_state().assert_populated("large warm phase");

        let warm = fixture.run_cached(self.context, "cached-warm.txt")?;

        println!("large: baseline {}", initial_baseline);
        println!("large: cached   {}", warm.hash());

        assert_eq!(
            &initial_baseline,
            warm.hash(),
            "large warm-cache output differs from cargo-about",
        );

        let warm_stats = warm.license_cache_stats()?;
        println!(
            "large: license cache {} hit, {} miss",
            warm_stats.hits, warm_stats.misses,
        );

        CacheAssertions::warm(&fixture, &warm)?;

        println!("large: phase 2 passed");

        /*
         * Phase 3
         *
         * Keep existing caches but update iroh.
         *
         * Metadata:
         *     Cargo.lock changed -> cache miss
         *
         * Licenses:
         *     unchanged packages -> cache hit
         *     changed/new packages -> cache miss
         */

        println!();
        println!("large: phase 3 - dependency update");

        let cache_before_update = fixture.cache_state();
        cache_before_update.assert_populated("large version-change phase");

        println!("large: updating iroh 0.90.0 -> 1.3.0");
        fixture.replace_file_text(
            Path::new("member").join("Cargo.toml"),
            OLD_IROH_DEPENDENCY,
            NEW_IROH_DEPENDENCY,
        )?;

        println!("large: updating Cargo.lock");
        fixture.update_package("iroh", NEW_IROH_VERSION)?;

        let cache_after_update = fixture.cache_state();

        assert_eq!(
            cache_before_update, cache_after_update,
            "changing dependency versions must not modify existing cache files",
        );

        println!("large: generating updated baseline");
        let updated_baseline = fixture.generate_baseline(self.context, "baseline-updated.txt")?;

        assert_ne!(
            initial_baseline, updated_baseline,
            "iroh update did not change generated cargo-about output",
        );

        println!("large: running cargo-about-cached with existing cache");
        let updated = fixture.run_cached(self.context, "cached-updated.txt")?;

        println!("large: baseline {}", updated_baseline);
        println!("large: cached   {}", updated.hash());

        assert_eq!(
            &updated_baseline,
            updated.hash(),
            "large dependency-update output differs from cargo-about",
        );

        let updated_stats = updated.license_cache_stats()?;
        println!(
            "large: license cache {} hit, {} miss",
            updated_stats.hits, updated_stats.misses,
        );

        CacheAssertions::version_change(&fixture, &updated)?;

        println!("large: phase 3 passed");

        println!();
        println!("large: success");

        Ok(())
    }
}
