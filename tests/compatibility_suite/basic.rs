use anyhow::Result;

use super::support::{CacheAssertions, CompatibilityContext, FixtureWorkspace};

pub(crate) struct BasicScenario<'a> {
    context: &'a CompatibilityContext,
}

impl<'a> BasicScenario<'a> {
    pub(crate) fn new(context: &'a CompatibilityContext) -> Self {
        Self { context }
    }

    pub(crate) fn run(&self) -> Result<()> {
        println!("basic: creating fixture");
        let fixture = FixtureWorkspace::create(self.context, "basic")?;

        println!("basic: generating Cargo.lock");
        fixture.generate_lockfile()?;

        println!("basic: generating cargo-about baseline");
        let baseline = fixture.generate_baseline(self.context, "baseline.txt")?;

        fixture.cache_state().assert_empty("basic cold phase");

        println!("basic: running cargo-about-cached (cold)");
        let cold = fixture.run_cached(self.context, "cached-cold.txt")?;

        println!("basic: baseline {}", baseline);
        println!("basic: cold     {}", cold.hash());

        assert_eq!(
            &baseline,
            cold.hash(),
            "basic cold-cache output differs from cargo-about",
        );

        CacheAssertions::cold(&fixture, &cold)?;
        println!("basic: cold cache matches");

        fixture.cache_state().assert_populated("basic warm phase");

        println!("basic: running cargo-about-cached (warm)");
        let warm = fixture.run_cached(self.context, "cached-warm.txt")?;

        println!("basic: warm     {}", warm.hash());

        assert_eq!(
            &baseline,
            warm.hash(),
            "basic warm-cache output differs from cargo-about",
        );

        CacheAssertions::warm(&fixture, &warm)?;
        println!("basic: warm cache matches");
        println!("basic: success");

        Ok(())
    }
}
